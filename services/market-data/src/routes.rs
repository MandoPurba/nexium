use actix_web::{HttpResponse, get, web};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use utoipa::ToSchema;

use nexium_core::error::ApiError;

use crate::repository;

// ---------------------------------------------------------------------------
// GET /market/ohlcv
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, ToSchema)]
pub struct OhlcvQuery {
    #[schema(example = "BTC/USDT")]
    pub pair: String,
    #[serde(default = "default_interval")]
    #[schema(example = "1h")]
    pub interval: String,
    pub limit: Option<i64>,
}

fn default_interval() -> String {
    "1h".into()
}

const VALID_INTERVALS: &[&str] = &["1m", "5m", "15m", "1h", "4h", "1d"];

#[derive(Debug, Serialize, ToSchema)]
pub struct OhlcvResponse {
    #[schema(example = "BTC/USDT")]
    pub pair: String,
    #[schema(example = "1h")]
    pub interval: String,
    #[schema(value_type = String, example = "65000")]
    pub open: Decimal,
    #[schema(value_type = String, example = "65500")]
    pub high: Decimal,
    #[schema(value_type = String, example = "64800")]
    pub low: Decimal,
    #[schema(value_type = String, example = "65200")]
    pub close: Decimal,
    #[schema(value_type = String, example = "12.5")]
    pub volume: Decimal,
    pub bucket: DateTime<Utc>,
}

#[utoipa::path(
    get,
    path = "/market/ohlcv",
    tag = "Market Data",
    params(
        ("pair" = String, Query, description = "Trading pair symbol"),
        ("interval" = String, Query, description = "Candle interval: 1m, 5m, 15m, 1h, 4h, 1d"),
        ("limit" = Option<i64>, Query, description = "Max candles (default 100, max 1000)"),
    ),
    responses(
        (status = 200, description = "OHLCV candles", body = Vec<OhlcvResponse>),
        (status = 400, description = "Invalid interval", body = ErrorResponse),
    )
)]
#[get("/market/ohlcv")]
#[tracing::instrument(name = "market.ohlcv", skip_all)]
pub async fn get_ohlcv(
    ts_pool: web::Data<TimescalePool>,
    query: web::Query<OhlcvQuery>,
) -> Result<HttpResponse, ApiError> {
    if !VALID_INTERVALS.contains(&query.interval.as_str()) {
        return Err(ApiError::Validation({
            let mut errors = validator::ValidationErrors::new();
            errors.add(
                "interval",
                validator::ValidationError::new("invalid_interval"),
            );
            errors
        }));
    }

    let pair = query.pair.to_uppercase();
    let limit = query.limit.unwrap_or(100).min(1000);

    let rows = repository::list_ohlcv(&ts_pool.0, &pair, &query.interval, limit)
        .await
        .map_err(|e| ApiError::Internal(e.into()))?;

    let body: Vec<OhlcvResponse> = rows
        .into_iter()
        .map(|r| OhlcvResponse {
            pair: r.pair,
            interval: r.interval,
            open: r.open,
            high: r.high,
            low: r.low,
            close: r.close,
            volume: r.volume,
            bucket: r.bucket,
        })
        .collect();

    Ok(HttpResponse::Ok().json(body))
}

// ---------------------------------------------------------------------------
// GET /market/orderbook/{pair}
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, ToSchema)]
pub struct OrderBookResponse {
    #[schema(example = "BTC/USDT")]
    pub pair: String,
    pub bids: serde_json::Value,
    pub asks: serde_json::Value,
    pub timestamp: DateTime<Utc>,
}

#[utoipa::path(
    get,
    path = "/market/orderbook/{pair}",
    tag = "Market Data",
    params(
        ("pair" = String, Path, description = "Trading pair symbol (e.g. BTC/USDT)")
    ),
    responses(
        (status = 200, description = "Current orderbook snapshot", body = OrderBookResponse),
        (status = 404, description = "No snapshot available", body = ErrorResponse),
    )
)]
#[get("/market/orderbook/{pair}")]
#[tracing::instrument(name = "market.orderbook", skip_all, fields(pair = %path))]
pub async fn get_orderbook(
    ts_pool: web::Data<TimescalePool>,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    let pair = path.into_inner().to_uppercase();

    let snap = repository::latest_snapshot(&ts_pool.0, &pair)
        .await
        .map_err(|e| ApiError::Internal(e.into()))?
        .ok_or_else(|| ApiError::NotFound(format!("no orderbook snapshot for '{pair}'")))?;

    Ok(HttpResponse::Ok().json(OrderBookResponse {
        pair: snap.pair,
        bids: snap.bids,
        asks: snap.asks,
        timestamp: snap.captured_at,
    }))
}

// ---------------------------------------------------------------------------
// GET /market/trades/{pair}
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, ToSchema)]
pub struct TradeResponse {
    pub id: uuid::Uuid,
    #[schema(example = "BTC/USDT")]
    pub pair: String,
    #[schema(value_type = String, example = "65000")]
    pub price: Decimal,
    #[schema(value_type = String, example = "0.01")]
    pub quantity: Decimal,
    #[schema(example = "buy")]
    pub side: String,
    pub executed_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct TradesQuery {
    pub limit: Option<i64>,
}

#[utoipa::path(
    get,
    path = "/market/trades/{pair}",
    tag = "Market Data",
    params(
        ("pair" = String, Path, description = "Trading pair symbol"),
        ("limit" = Option<i64>, Query, description = "Max trades (default 50, max 200)"),
    ),
    responses(
        (status = 200, description = "Recent trades", body = Vec<TradeResponse>),
    )
)]
#[get("/market/trades/{pair}")]
#[tracing::instrument(name = "market.trades", skip_all, fields(pair = %path))]
pub async fn get_trades(
    pg_pool: web::Data<PgPool>,
    path: web::Path<String>,
    query: web::Query<TradesQuery>,
) -> Result<HttpResponse, ApiError> {
    let pair = path.into_inner().to_uppercase();
    let limit = query.limit.unwrap_or(50).min(200);

    let rows = repository::recent_trades(pg_pool.get_ref(), &pair, limit)
        .await
        .map_err(|e| ApiError::Internal(e.into()))?;

    let body: Vec<TradeResponse> = rows
        .into_iter()
        .map(|r| TradeResponse {
            id: r.id,
            pair: r.pair,
            price: r.price,
            quantity: r.quantity,
            side: r.side,
            executed_at: r.executed_at,
        })
        .collect();

    Ok(HttpResponse::Ok().json(body))
}

// ---------------------------------------------------------------------------
// Newtype wrapper so we can distinguish pg_pool vs ts_pool in app_data
// ---------------------------------------------------------------------------

pub struct TimescalePool(pub PgPool);

// ---- Error response schema for OpenAPI ------------------------------------

#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorResponse {
    #[schema(example = "VALIDATION_ERROR")]
    pub code: String,
    #[schema(example = "request validation failed")]
    pub message: String,
    #[schema(nullable)]
    pub details: Option<serde_json::Value>,
}
