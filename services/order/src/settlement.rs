//! Trade settlement — applies the persistence and wallet effects of trades
//! emitted by the matching engine.
//!
//! Subscribes to [`EngineEvent`]s on an mpsc channel and, for each trade,
//! inside a single DB transaction:
//!
//! 1. Inserts a row into `trading.trades`.
//! 2. Updates `filled_qty` and `status` for both maker and taker orders.
//! 3. Settles wallets — see [`apply_trade_to_wallets`] for the details.

use anyhow::Context as _;
use nexium_matching_engine::{EngineEvent, Side, Trade};
use rust_decimal::Decimal;
use sqlx::{PgPool, Postgres, Transaction};
use tokio::sync::mpsc;
use uuid::Uuid;

/// Subscribe to engine events and settle them. Runs until the sender is dropped.
pub async fn run(pool: PgPool, mut event_rx: mpsc::Receiver<EngineEvent>) {
    while let Some(event) = event_rx.recv().await {
        match event {
            EngineEvent::OrderProcessed { taker: _, trades } => {
                for trade in trades {
                    if let Err(err) = settle_trade(&pool, &trade).await {
                        tracing::error!(
                            error = %err,
                            trade_id = %trade.id,
                            "failed to settle trade"
                        );
                    }
                }
            }
            EngineEvent::OrderCancelled { .. } => {
                // Cancellation effects are handled synchronously by the
                // HTTP DELETE handler — engine-side cancel is a no-op here
                // beyond keeping the book consistent.
            }
        }
    }

    tracing::info!("settlement task event channel closed; shutting down");
}

async fn settle_trade(pool: &PgPool, trade: &Trade) -> anyhow::Result<()> {
    let mut tx = pool.begin().await.context("begin trade settlement tx")?;

    // 1. Insert trade record.
    sqlx::query(
        r#"
        INSERT INTO trading.trades
            (id, maker_order_id, taker_order_id, pair, price, quantity, executed_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        "#,
    )
    .bind(trade.id)
    .bind(trade.maker_order_id)
    .bind(trade.taker_order_id)
    .bind(&trade.pair)
    .bind(trade.price)
    .bind(trade.quantity)
    .bind(trade.executed_at)
    .execute(&mut *tx)
    .await
    .context("insert trade row")?;

    // 2. Update filled_qty + status on both sides.
    update_order_fill(&mut tx, trade.maker_order_id, trade.quantity).await?;
    update_order_fill(&mut tx, trade.taker_order_id, trade.quantity).await?;

    // 3. Settle wallets.
    apply_trade_to_wallets(&mut tx, trade).await?;

    tx.commit().await.context("commit trade settlement")?;

    tracing::info!(
        trade_id = %trade.id,
        pair = %trade.pair,
        qty = %trade.quantity,
        price = %trade.price,
        "trade settled"
    );

    Ok(())
}

/// Add `fill` to the order's `filled_qty` and update `status` accordingly.
async fn update_order_fill(
    tx: &mut Transaction<'_, Postgres>,
    order_id: Uuid,
    fill: Decimal,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        UPDATE trading.orders
        SET filled_qty = filled_qty + $1,
            status = CASE
                WHEN filled_qty + $1 >= quantity THEN 'filled'::trading.order_status
                ELSE 'partially_filled'::trading.order_status
            END,
            updated_at = NOW()
        WHERE id = $2
        "#,
    )
    .bind(fill)
    .bind(order_id)
    .execute(&mut **tx)
    .await
    .context("update order fill")?;
    Ok(())
}

/// Wallet effects per trade. Both sides hit the wallets table inside the same
/// transaction as the trade insert, so there's no window where a trade exists
/// without its balance impact.
///
/// Conventions:
///
/// * Limit buys lock `order.price × qty` in quote at placement. If the trade
///   executes at a lower maker price, the unused portion is released back to
///   available — `(buyer_lock_price - trade_price) × qty`.
/// * Sells (limit and market) lock `qty` in base at placement.
/// * Market buys do not lock at placement; the matched amount is debited
///   directly from `balance` (available) at settle time. Sprint 5 doesn't
///   verify market-buy balance at placement — that's a Sprint 7 hardening.
async fn apply_trade_to_wallets(
    tx: &mut Transaction<'_, Postgres>,
    trade: &Trade,
) -> anyhow::Result<()> {
    // Look up the pair so we know which currency is which.
    let (base, quote): (String, String) =
        sqlx::query_as("SELECT base_currency, quote_currency FROM trading.pairs WHERE symbol = $1")
            .bind(&trade.pair)
            .fetch_one(&mut **tx)
            .await
            .context("look up pair currencies")?;

    // Reload both orders so we know how much was originally locked.
    let maker = load_order_for_settlement(tx, trade.maker_order_id).await?;
    let taker = load_order_for_settlement(tx, trade.taker_order_id).await?;

    let (buyer, seller) = match trade.maker_side {
        Side::Buy => (maker, taker),
        Side::Sell => (taker, maker),
    };

    // --- Buyer ----------------------------------------------------------
    if buyer.order_type == "limit" {
        // limit buy: lock_per_qty = buyer.price; release excess back to available.
        let buyer_price = buyer
            .price
            .context("limit buy without price — invariant broken")?;
        let original_lock = buyer_price * trade.quantity;
        let trade_value = trade.price * trade.quantity;
        let refund = original_lock - trade_value;

        // Debit locked + refund the excess to available.
        sqlx::query(
            r#"
            UPDATE wallet.wallets
            SET locked_balance = locked_balance - $1,
                balance        = balance + $2,
                updated_at     = NOW()
            WHERE user_id = $3 AND currency = $4
            "#,
        )
        .bind(original_lock)
        .bind(refund)
        .bind(buyer.user_id)
        .bind(&quote)
        .execute(&mut **tx)
        .await
        .context("buyer quote settlement")?;
    } else {
        // market buy: nothing was locked; debit available directly.
        let trade_value = trade.price * trade.quantity;
        sqlx::query(
            r#"
            UPDATE wallet.wallets
            SET balance    = balance - $1,
                updated_at = NOW()
            WHERE user_id = $2 AND currency = $3
            "#,
        )
        .bind(trade_value)
        .bind(buyer.user_id)
        .bind(&quote)
        .execute(&mut **tx)
        .await
        .context("buyer market quote settlement")?;
    }

    // Credit buyer's base currency.
    sqlx::query(
        r#"
        UPDATE wallet.wallets
        SET balance    = balance + $1,
            updated_at = NOW()
        WHERE user_id = $2 AND currency = $3
        "#,
    )
    .bind(trade.quantity)
    .bind(buyer.user_id)
    .bind(&base)
    .execute(&mut **tx)
    .await
    .context("buyer base credit")?;

    // Insert wallet_txns rows for the buyer leg (trade_debit + trade_credit).
    record_wallet_txn(tx, buyer.user_id, &quote, trade, "trade_debit").await?;
    record_wallet_txn(tx, buyer.user_id, &base, trade, "trade_credit").await?;

    // --- Seller ---------------------------------------------------------
    // All sell variants lock `qty` in base at placement (Sprint 4).
    sqlx::query(
        r#"
        UPDATE wallet.wallets
        SET locked_balance = locked_balance - $1,
            updated_at     = NOW()
        WHERE user_id = $2 AND currency = $3
        "#,
    )
    .bind(trade.quantity)
    .bind(seller.user_id)
    .bind(&base)
    .execute(&mut **tx)
    .await
    .context("seller base unlock")?;

    let trade_value = trade.price * trade.quantity;
    sqlx::query(
        r#"
        UPDATE wallet.wallets
        SET balance    = balance + $1,
            updated_at = NOW()
        WHERE user_id = $2 AND currency = $3
        "#,
    )
    .bind(trade_value)
    .bind(seller.user_id)
    .bind(&quote)
    .execute(&mut **tx)
    .await
    .context("seller quote credit")?;

    record_wallet_txn(tx, seller.user_id, &base, trade, "trade_debit").await?;
    record_wallet_txn(tx, seller.user_id, &quote, trade, "trade_credit").await?;

    Ok(())
}

struct SettlementOrder {
    user_id: Uuid,
    order_type: String,
    price: Option<Decimal>,
}

async fn load_order_for_settlement(
    tx: &mut Transaction<'_, Postgres>,
    order_id: Uuid,
) -> anyhow::Result<SettlementOrder> {
    let row: (Uuid, String, Option<Decimal>) = sqlx::query_as(
        r#"
        SELECT user_id, type::text AS order_type, price
        FROM trading.orders
        WHERE id = $1
        "#,
    )
    .bind(order_id)
    .fetch_one(&mut **tx)
    .await
    .context("load order for settlement")?;

    Ok(SettlementOrder {
        user_id: row.0,
        order_type: row.1,
        price: row.2,
    })
}

async fn record_wallet_txn(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    currency: &str,
    trade: &Trade,
    txn_type: &str,
) -> anyhow::Result<()> {
    // The amount stored is in the currency the txn affects:
    //   trade_debit  (quote, buyer) = trade_value
    //   trade_credit (base,  buyer) = trade.quantity
    //   trade_debit  (base,  seller) = trade.quantity
    //   trade_credit (quote, seller) = trade_value
    let amount = if currency == trade.pair.split('/').next().unwrap_or("") {
        // base currency
        trade.quantity
    } else {
        trade.price * trade.quantity
    };

    sqlx::query(
        r#"
        INSERT INTO wallet.wallet_txns
            (wallet_id, type, amount, status, ref_id)
        SELECT id, $2::wallet.txn_type, $3, 'confirmed'::wallet.txn_status, $4
        FROM wallet.wallets
        WHERE user_id = $1 AND currency = $5
        "#,
    )
    .bind(user_id)
    .bind(txn_type)
    .bind(amount)
    .bind(trade.id)
    .bind(currency)
    .execute(&mut **tx)
    .await
    .context("insert wallet_txn")?;
    Ok(())
}
