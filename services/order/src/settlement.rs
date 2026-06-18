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
use chrono::{DateTime, NaiveTime, Timelike, Utc};
use nexium_matching_engine::{EngineEvent, OrderBookSnapshot, Side, Trade};
use rust_decimal::Decimal;
use sqlx::{PgPool, Postgres, Transaction};
use tokio::sync::mpsc;
use uuid::Uuid;

const OHLCV_INTERVALS: &[&str] = &["1m", "5m", "15m", "1h", "4h", "1d"];

/// Subscribe to engine events and settle them. Runs until the sender is dropped.
pub async fn run(
    pool: PgPool,
    ts_pool: Option<PgPool>,
    nats: Option<async_nats::Client>,
    mut event_rx: mpsc::Receiver<EngineEvent>,
) {
    while let Some(event) = event_rx.recv().await {
        match event {
            EngineEvent::OrderProcessed {
                taker: _,
                trades,
                book_snapshot,
            } => {
                for trade in &trades {
                    if let Err(err) = settle_trade(&pool, trade).await {
                        tracing::error!(
                            error = %err,
                            trade_id = %trade.id,
                            "failed to settle trade"
                        );
                    }
                    if let Some(ref ts) = ts_pool {
                        if let Err(err) = upsert_ohlcv(ts, trade).await {
                            tracing::error!(
                                error = %err,
                                trade_id = %trade.id,
                                "failed to upsert ohlcv"
                            );
                        }
                    }
                    if let Some(ref nc) = nats {
                        publish_trade(nc, trade).await;
                        publish_order_status(nc, &pool, trade).await;
                    }
                }
                if let Some(ref nc) = nats {
                    publish_orderbook(nc, &book_snapshot).await;
                }
            }
            EngineEvent::OrderCancelled {
                book_snapshot,
                order_id: _,
                removed: _,
            } => {
                if let Some(ref nc) = nats {
                    publish_orderbook(nc, &book_snapshot).await;
                }
            }
        }
    }

    tracing::info!("settlement task event channel closed; shutting down");
}

// ---------------------------------------------------------------------------
// NATS publishing
// ---------------------------------------------------------------------------

fn nats_pair(pair: &str) -> String {
    pair.replace('/', "-")
}

async fn publish_trade(nc: &async_nats::Client, trade: &Trade) {
    let topic = format!("nexium.trades.{}", nats_pair(&trade.pair));
    let payload = serde_json::json!({
        "id": trade.id,
        "pair": trade.pair,
        "price": trade.price.to_string(),
        "quantity": trade.quantity.to_string(),
        "maker_side": format!("{:?}", trade.maker_side).to_lowercase(),
        "executed_at": trade.executed_at.to_rfc3339(),
    });
    if let Err(err) = nc.publish(topic.clone(), payload.to_string().into()).await {
        tracing::warn!(error = %err, topic, "nats publish trade failed");
    }
}

async fn publish_order_status(nc: &async_nats::Client, pool: &PgPool, trade: &Trade) {
    for (label, order_id, user_id) in [
        ("maker", trade.maker_order_id, trade.maker_user_id),
        ("taker", trade.taker_order_id, trade.taker_user_id),
    ] {
        let row: Option<(String, String, Decimal, Decimal)> = sqlx::query_as(
            "SELECT pair, status::text, filled_qty, quantity FROM trading.orders WHERE id = $1",
        )
        .bind(order_id)
        .fetch_optional(pool)
        .await
        .unwrap_or(None);

        if let Some((pair, status, filled_qty, quantity)) = row {
            let topic = format!("nexium.orders.{user_id}");
            let payload = serde_json::json!({
                "id": order_id,
                "pair": pair,
                "role": label,
                "status": status,
                "filled_qty": filled_qty.to_string(),
                "quantity": quantity.to_string(),
            });
            let _ = nc.publish(topic, payload.to_string().into()).await;
        }
    }
}

async fn publish_orderbook(nc: &async_nats::Client, snap: &OrderBookSnapshot) {
    let topic = format!("nexium.orderbook.{}", nats_pair(&snap.pair));
    let payload = serde_json::json!({
        "pair": snap.pair,
        "bids": snap.bids.iter().map(|(p, q)| [p.to_string(), q.to_string()]).collect::<Vec<_>>(),
        "asks": snap.asks.iter().map(|(p, q)| [p.to_string(), q.to_string()]).collect::<Vec<_>>(),
    });
    let _ = nc.publish(topic, payload.to_string().into()).await;
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

    // 4. Calculate and insert fees.
    apply_fees(&mut tx, trade).await?;

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

// ---------------------------------------------------------------------------
// Fee calculation
// ---------------------------------------------------------------------------

async fn apply_fees(tx: &mut Transaction<'_, Postgres>, trade: &Trade) -> anyhow::Result<()> {
    let tier: Option<(Decimal, Decimal)> =
        sqlx::query_as("SELECT maker_rate, taker_rate FROM fee.fee_tiers WHERE level = 'standard'")
            .fetch_optional(&mut **tx)
            .await
            .context("look up fee tier")?;

    let (maker_rate, taker_rate) = match tier {
        Some(t) => t,
        None => return Ok(()),
    };

    let trade_value = trade.price * trade.quantity;

    let (_, quote): (String, String) =
        sqlx::query_as("SELECT base_currency, quote_currency FROM trading.pairs WHERE symbol = $1")
            .bind(&trade.pair)
            .fetch_one(&mut **tx)
            .await
            .context("look up pair for fees")?;

    let maker_fee = maker_rate * trade_value;
    let taker_fee = taker_rate * trade_value;

    let maker_user: Uuid = sqlx::query_scalar("SELECT user_id FROM trading.orders WHERE id = $1")
        .bind(trade.maker_order_id)
        .fetch_one(&mut **tx)
        .await
        .context("look up maker user")?;

    let taker_user: Uuid = sqlx::query_scalar("SELECT user_id FROM trading.orders WHERE id = $1")
        .bind(trade.taker_order_id)
        .fetch_one(&mut **tx)
        .await
        .context("look up taker user")?;

    for (user_id, fee_amount, fee_rate, fee_type) in [
        (maker_user, maker_fee, maker_rate, "maker"),
        (taker_user, taker_fee, taker_rate, "taker"),
    ] {
        sqlx::query(
            r#"
            INSERT INTO fee.fees (user_id, trade_id, currency, amount, rate, type)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(user_id)
        .bind(trade.id)
        .bind(&quote)
        .bind(fee_amount)
        .bind(fee_rate)
        .bind(fee_type)
        .execute(&mut **tx)
        .await
        .context("insert fee record")?;

        sqlx::query(
            r#"
            UPDATE wallet.wallets
            SET balance = balance - $1, updated_at = NOW()
            WHERE user_id = $2 AND currency = $3
            "#,
        )
        .bind(fee_amount)
        .bind(user_id)
        .bind(&quote)
        .execute(&mut **tx)
        .await
        .context("deduct fee from wallet")?;

        record_wallet_txn_fee(tx, user_id, &quote, fee_amount, trade.id).await?;
    }

    tracing::debug!(
        trade_id = %trade.id,
        maker_fee = %maker_fee,
        taker_fee = %taker_fee,
        "fees applied"
    );

    Ok(())
}

async fn record_wallet_txn_fee(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    currency: &str,
    amount: Decimal,
    trade_id: Uuid,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        INSERT INTO wallet.wallet_txns
            (wallet_id, type, amount, status, ref_id)
        SELECT id, 'fee'::wallet.txn_type, $2, 'confirmed'::wallet.txn_status, $3
        FROM wallet.wallets
        WHERE user_id = $1 AND currency = $4
        "#,
    )
    .bind(user_id)
    .bind(amount)
    .bind(trade_id)
    .bind(currency)
    .execute(&mut **tx)
    .await
    .context("insert fee wallet_txn")?;
    Ok(())
}

// ---------------------------------------------------------------------------
// OHLCV aggregation
// ---------------------------------------------------------------------------

async fn upsert_ohlcv(ts_pool: &PgPool, trade: &Trade) -> anyhow::Result<()> {
    for interval in OHLCV_INTERVALS {
        let bucket = floor_to_interval(trade.executed_at, interval);
        sqlx::query(
            r#"
            INSERT INTO market.ohlcv (pair, interval, open, high, low, close, volume, bucket)
            VALUES ($1, $2, $3, $3, $3, $3, $4, $5)
            ON CONFLICT (pair, interval, bucket) DO UPDATE
            SET high   = GREATEST(market.ohlcv.high, EXCLUDED.high),
                low    = LEAST(market.ohlcv.low, EXCLUDED.low),
                close  = EXCLUDED.close,
                volume = market.ohlcv.volume + EXCLUDED.volume
            "#,
        )
        .bind(&trade.pair)
        .bind(interval)
        .bind(trade.price)
        .bind(trade.quantity)
        .bind(bucket)
        .execute(ts_pool)
        .await
        .context("upsert ohlcv")?;
    }
    Ok(())
}

fn floor_to_interval(ts: DateTime<Utc>, interval: &str) -> DateTime<Utc> {
    match interval {
        "1m" => floor_minutes(ts, 1),
        "5m" => floor_minutes(ts, 5),
        "15m" => floor_minutes(ts, 15),
        "1h" => floor_minutes(ts, 60),
        "4h" => floor_minutes(ts, 240),
        "1d" => ts.date_naive().and_time(NaiveTime::MIN).and_utc(),
        _ => ts,
    }
}

fn floor_minutes(ts: DateTime<Utc>, mins: u32) -> DateTime<Utc> {
    let total_minutes = ts.hour() * 60 + ts.minute();
    let floored = (total_minutes / mins) * mins;
    ts.date_naive()
        .and_hms_opt(floored / 60, floored % 60, 0)
        .unwrap()
        .and_utc()
}

// ---------------------------------------------------------------------------
// Wallet transaction logging
// ---------------------------------------------------------------------------

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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn floor_1m() {
        let ts = Utc.with_ymd_and_hms(2026, 6, 17, 14, 23, 45).unwrap();
        let b = floor_to_interval(ts, "1m");
        assert_eq!(b, Utc.with_ymd_and_hms(2026, 6, 17, 14, 23, 0).unwrap());
    }

    #[test]
    fn floor_5m() {
        let ts = Utc.with_ymd_and_hms(2026, 6, 17, 14, 23, 45).unwrap();
        let b = floor_to_interval(ts, "5m");
        assert_eq!(b, Utc.with_ymd_and_hms(2026, 6, 17, 14, 20, 0).unwrap());
    }

    #[test]
    fn floor_15m() {
        let ts = Utc.with_ymd_and_hms(2026, 6, 17, 14, 23, 45).unwrap();
        let b = floor_to_interval(ts, "15m");
        assert_eq!(b, Utc.with_ymd_and_hms(2026, 6, 17, 14, 15, 0).unwrap());
    }

    #[test]
    fn floor_1h() {
        let ts = Utc.with_ymd_and_hms(2026, 6, 17, 14, 23, 45).unwrap();
        let b = floor_to_interval(ts, "1h");
        assert_eq!(b, Utc.with_ymd_and_hms(2026, 6, 17, 14, 0, 0).unwrap());
    }

    #[test]
    fn floor_4h() {
        let ts = Utc.with_ymd_and_hms(2026, 6, 17, 14, 23, 45).unwrap();
        let b = floor_to_interval(ts, "4h");
        assert_eq!(b, Utc.with_ymd_and_hms(2026, 6, 17, 12, 0, 0).unwrap());
    }

    #[test]
    fn floor_1d() {
        let ts = Utc.with_ymd_and_hms(2026, 6, 17, 14, 23, 45).unwrap();
        let b = floor_to_interval(ts, "1d");
        assert_eq!(b, Utc.with_ymd_and_hms(2026, 6, 17, 0, 0, 0).unwrap());
    }
}
