use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use sqlx::PgPool;

// ---------------------------------------------------------------------------
// OHLCV
// ---------------------------------------------------------------------------

#[derive(Debug, sqlx::FromRow)]
pub struct OhlcvRow {
    pub pair: String,
    pub interval: String,
    pub open: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub close: Decimal,
    pub volume: Decimal,
    pub bucket: DateTime<Utc>,
}

pub async fn list_ohlcv(
    ts_pool: &PgPool,
    pair: &str,
    interval: &str,
    limit: i64,
) -> sqlx::Result<Vec<OhlcvRow>> {
    sqlx::query_as::<_, OhlcvRow>(
        r#"
        SELECT pair, interval, open, high, low, close, volume, bucket
        FROM market.ohlcv
        WHERE pair = $1 AND interval = $2
        ORDER BY bucket DESC
        LIMIT $3
        "#,
    )
    .bind(pair)
    .bind(interval)
    .bind(limit)
    .fetch_all(ts_pool)
    .await
}

// ---------------------------------------------------------------------------
// Orderbook snapshot
// ---------------------------------------------------------------------------

#[derive(Debug, sqlx::FromRow)]
pub struct SnapshotRow {
    pub pair: String,
    pub bids: serde_json::Value,
    pub asks: serde_json::Value,
    pub captured_at: DateTime<Utc>,
}

pub async fn latest_snapshot(ts_pool: &PgPool, pair: &str) -> sqlx::Result<Option<SnapshotRow>> {
    sqlx::query_as::<_, SnapshotRow>(
        r#"
        SELECT pair, bids, asks, captured_at
        FROM market.order_book_snapshots
        WHERE pair = $1
        ORDER BY captured_at DESC
        LIMIT 1
        "#,
    )
    .bind(pair)
    .fetch_optional(ts_pool)
    .await
}

// ---------------------------------------------------------------------------
// Recent trades (from postgres trading.trades)
// ---------------------------------------------------------------------------

#[derive(Debug, sqlx::FromRow)]
pub struct TradeRow {
    pub id: uuid::Uuid,
    pub pair: String,
    pub price: Decimal,
    pub quantity: Decimal,
    pub side: String,
    pub executed_at: DateTime<Utc>,
}

pub async fn recent_trades(
    pg_pool: &PgPool,
    pair: &str,
    limit: i64,
) -> sqlx::Result<Vec<TradeRow>> {
    sqlx::query_as::<_, TradeRow>(
        r#"
        SELECT
            t.id,
            t.pair,
            t.price,
            t.quantity,
            o.side::text AS side,
            t.executed_at
        FROM trading.trades t
        JOIN trading.orders o ON o.id = t.taker_order_id
        WHERE t.pair = $1
        ORDER BY t.executed_at DESC
        LIMIT $2
        "#,
    )
    .bind(pair)
    .bind(limit)
    .fetch_all(pg_pool)
    .await
}
