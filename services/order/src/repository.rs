//! Persistence layer for `trading.orders` and `trading.pairs`.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use sqlx::PgPool;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Records
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PairRecord {
    pub id: Uuid,
    pub symbol: String,
    pub base_currency: String,
    pub quote_currency: String,
    pub min_qty: Decimal,
    pub tick_size: Decimal,
    pub is_active: bool,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct OrderRecord {
    pub id: Uuid,
    pub user_id: Uuid,
    pub pair: String,
    pub side: String,
    pub order_type: String,
    pub status: String,
    pub price: Option<Decimal>,
    pub quantity: Decimal,
    pub filled_qty: Decimal,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Eligibility check
// ---------------------------------------------------------------------------

/// Returns `Ok(true)` when the user's account is `active` and their latest
/// approved KYC record is at least `basic`. Callers map `false` → 403.
pub async fn check_trading_eligible(pool: &PgPool, user_id: Uuid) -> Result<bool, sqlx::Error> {
    #[derive(sqlx::FromRow)]
    struct EligibilityRow {
        status: String,
        kyc_level: String,
    }

    let row = sqlx::query_as::<_, EligibilityRow>(
        r#"
        SELECT
            u.status::text AS status,
            COALESCE(
                (
                    SELECT k.level::text
                    FROM auth.kyc k
                    WHERE k.user_id = u.id AND k.status = 'approved'
                    ORDER BY k.created_at DESC
                    LIMIT 1
                ),
                'none'
            ) AS kyc_level
        FROM auth.users u
        WHERE u.id = $1
        "#,
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?;

    Ok(match row {
        None => false,
        Some(r) => r.status == "active" && matches!(r.kyc_level.as_str(), "basic" | "advanced"),
    })
}

// ---------------------------------------------------------------------------
// Pairs
// ---------------------------------------------------------------------------

pub async fn list_pairs(pool: &PgPool) -> Result<Vec<PairRecord>, sqlx::Error> {
    sqlx::query_as::<_, PairRecord>(
        r#"
        SELECT id, symbol, base_currency, quote_currency, min_qty, tick_size, is_active
        FROM trading.pairs
        WHERE is_active = TRUE
        ORDER BY symbol
        "#,
    )
    .fetch_all(pool)
    .await
}

pub async fn find_pair(pool: &PgPool, symbol: &str) -> Result<Option<PairRecord>, sqlx::Error> {
    sqlx::query_as::<_, PairRecord>(
        r#"
        SELECT id, symbol, base_currency, quote_currency, min_qty, tick_size, is_active
        FROM trading.pairs
        WHERE symbol = $1
        "#,
    )
    .bind(symbol)
    .fetch_optional(pool)
    .await
}

// ---------------------------------------------------------------------------
// Orders
// ---------------------------------------------------------------------------

pub struct NewOrder<'a> {
    pub user_id: Uuid,
    pub pair: &'a str,
    pub side: &'a str,
    pub order_type: &'a str,
    pub price: Option<Decimal>,
    pub quantity: Decimal,
}

pub async fn insert_order(pool: &PgPool, o: NewOrder<'_>) -> Result<OrderRecord, sqlx::Error> {
    sqlx::query_as::<_, OrderRecord>(
        r#"
        INSERT INTO trading.orders (user_id, pair, side, type, price, quantity)
        VALUES ($1, $2, $3::trading.order_side, $4::trading.order_type, $5, $6)
        RETURNING
            id, user_id, pair,
            side::text       AS side,
            type::text       AS order_type,
            status::text     AS status,
            price, quantity, filled_qty, created_at, updated_at
        "#,
    )
    .bind(o.user_id)
    .bind(o.pair)
    .bind(o.side)
    .bind(o.order_type)
    .bind(o.price)
    .bind(o.quantity)
    .fetch_one(pool)
    .await
}

pub struct OrderFilter {
    pub pair: Option<String>,
    pub status: Option<String>,
    pub limit: i64,
    pub offset: i64,
}

pub async fn list_orders(
    pool: &PgPool,
    user_id: Uuid,
    filter: OrderFilter,
) -> Result<Vec<OrderRecord>, sqlx::Error> {
    // sqlx runtime queries don't support dynamic WHERE clauses well, so we
    // use a pattern that always binds all params and filters NULLs out.
    sqlx::query_as::<_, OrderRecord>(
        r#"
        SELECT
            id, user_id, pair,
            side::text       AS side,
            type::text       AS order_type,
            status::text     AS status,
            price, quantity, filled_qty, created_at, updated_at
        FROM trading.orders
        WHERE user_id = $1
          AND ($2::text IS NULL OR pair   = $2)
          AND ($3::text IS NULL OR status::text = $3)
        ORDER BY created_at DESC
        LIMIT $4 OFFSET $5
        "#,
    )
    .bind(user_id)
    .bind(filter.pair)
    .bind(filter.status)
    .bind(filter.limit)
    .bind(filter.offset)
    .fetch_all(pool)
    .await
}

pub async fn find_order(
    pool: &PgPool,
    order_id: Uuid,
    user_id: Uuid,
) -> Result<Option<OrderRecord>, sqlx::Error> {
    sqlx::query_as::<_, OrderRecord>(
        r#"
        SELECT
            id, user_id, pair,
            side::text       AS side,
            type::text       AS order_type,
            status::text     AS status,
            price, quantity, filled_qty, created_at, updated_at
        FROM trading.orders
        WHERE id = $1 AND user_id = $2
        "#,
    )
    .bind(order_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await
}

/// Atomically sets an order to `cancelled`. Returns the cancelled order, or
/// `None` if the order does not exist, does not belong to this user, or is
/// already in a terminal state (filled, cancelled, rejected).
pub async fn cancel_order(
    pool: &PgPool,
    order_id: Uuid,
    user_id: Uuid,
) -> Result<Option<OrderRecord>, sqlx::Error> {
    sqlx::query_as::<_, OrderRecord>(
        r#"
        UPDATE trading.orders
        SET status = 'cancelled', updated_at = NOW()
        WHERE id = $1
          AND user_id = $2
          AND status IN ('open', 'partially_filled')
        RETURNING
            id, user_id, pair,
            side::text       AS side,
            type::text       AS order_type,
            status::text     AS status,
            price, quantity, filled_qty, created_at, updated_at
        "#,
    )
    .bind(order_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await
}
