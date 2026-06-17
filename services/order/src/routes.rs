//! HTTP handlers for the order service.

use actix_web::{HttpResponse, delete, get, post, web};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;
use validator::{Validate, ValidationError};

use nexium_core::error::ApiError;
use nexium_core::extractors::AuthUser;

use crate::repository::{self, OrderFilter, OrderRecord};

// ---------------------------------------------------------------------------
// Shared response types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct PairResponse {
    pub symbol: String,
    pub base_currency: String,
    pub quote_currency: String,
    pub min_qty: Decimal,
    pub tick_size: Decimal,
}

#[derive(Debug, Serialize)]
pub struct OrderResponse {
    pub id: Uuid,
    pub pair: String,
    pub side: String,
    #[serde(rename = "type")]
    pub order_type: String,
    pub status: String,
    pub price: Option<Decimal>,
    pub quantity: Decimal,
    pub filled_qty: Decimal,
    pub created_at: DateTime<Utc>,
}

impl From<OrderRecord> for OrderResponse {
    fn from(r: OrderRecord) -> Self {
        Self {
            id: r.id,
            pair: r.pair,
            side: r.side,
            order_type: r.order_type,
            status: r.status,
            price: r.price,
            quantity: r.quantity,
            filled_qty: r.filled_qty,
            created_at: r.created_at,
        }
    }
}

// ---------------------------------------------------------------------------
// GET /pairs
// ---------------------------------------------------------------------------

#[get("/pairs")]
#[tracing::instrument(name = "order.list_pairs", skip_all)]
pub async fn list_pairs(pool: web::Data<PgPool>) -> Result<HttpResponse, ApiError> {
    let pairs = repository::list_pairs(pool.get_ref())
        .await
        .map_err(|e| ApiError::Internal(e.into()))?;

    let body: Vec<PairResponse> = pairs
        .into_iter()
        .map(|p| PairResponse {
            symbol: p.symbol,
            base_currency: p.base_currency,
            quote_currency: p.quote_currency,
            min_qty: p.min_qty,
            tick_size: p.tick_size,
        })
        .collect();

    Ok(HttpResponse::Ok().json(body))
}

// ---------------------------------------------------------------------------
// POST /orders
// ---------------------------------------------------------------------------

fn validate_positive(v: &Decimal) -> Result<(), ValidationError> {
    if *v > Decimal::ZERO {
        Ok(())
    } else {
        Err(ValidationError::new("must_be_positive"))
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OrderSide {
    Buy,
    Sell,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OrderType {
    Limit,
    Market,
}

#[derive(Debug, Deserialize, Validate)]
pub struct PlaceOrderRequest {
    #[validate(length(min = 3, max = 20))]
    pub pair: String,
    pub side: OrderSide,
    #[serde(rename = "type")]
    pub order_type: OrderType,
    // Price validation (must be positive when provided) is checked in the handler
    // body, alongside the limit-requires-price business rule.
    pub price: Option<Decimal>,
    #[validate(custom(function = "validate_positive"))]
    pub quantity: Decimal,
}

#[derive(Debug, Serialize)]
pub struct PlaceOrderResponse {
    pub id: Uuid,
    pub pair: String,
    pub side: String,
    #[serde(rename = "type")]
    pub order_type: String,
    pub status: String,
    pub price: Option<Decimal>,
    pub quantity: Decimal,
    pub filled_qty: Decimal,
    pub created_at: DateTime<Utc>,
}

#[post("/orders")]
#[tracing::instrument(name = "order.place", skip_all, fields(user_id = %user.id))]
pub async fn place_order(
    pool: web::Data<PgPool>,
    user: AuthUser,
    body: web::Json<PlaceOrderRequest>,
) -> Result<HttpResponse, ApiError> {
    let body = body.into_inner();
    body.validate()?;

    // Limit orders require a positive price; market orders must omit it.
    let price_invalid = matches!(&body.order_type, OrderType::Limit)
        && !matches!(body.price, Some(p) if p > Decimal::ZERO);
    if price_invalid {
        return Err(ApiError::Validation({
            let mut errors = validator::ValidationErrors::new();
            errors.add("price", ValidationError::new("required_for_limit_orders"));
            errors
        }));
    }

    // Role guard: active account + at least KYC basic.
    let eligible = repository::check_trading_eligible(pool.get_ref(), user.id)
        .await
        .map_err(|e| ApiError::Internal(e.into()))?;
    if !eligible {
        return Err(ApiError::Forbidden(
            "trading requires an active account with at least basic KYC".into(),
        ));
    }

    let pair_symbol = body.pair.to_uppercase();
    let pair = repository::find_pair(pool.get_ref(), &pair_symbol)
        .await
        .map_err(|e| ApiError::Internal(e.into()))?
        .ok_or_else(|| ApiError::NotFound(format!("pair '{pair_symbol}' not found")))?;

    if !pair.is_active {
        return Err(ApiError::PairInactive(format!(
            "pair '{pair_symbol}' is currently inactive"
        )));
    }

    let side_str = match body.side {
        OrderSide::Buy => "buy",
        OrderSide::Sell => "sell",
    };
    let type_str = match body.order_type {
        OrderType::Limit => "limit",
        OrderType::Market => "market",
    };

    // Lock balance before inserting the order so we don't create an open order
    // without the backing funds.
    //
    // buy  limit  → lock price × quantity in quote currency (e.g. USDT)
    // sell limit  → lock quantity in base currency (e.g. BTC)
    // buy  market → price unknown; skip lock (matching engine settles funds)
    // sell market → lock quantity in base currency
    let lock = match (&body.order_type, &body.side, body.price) {
        (OrderType::Limit, OrderSide::Buy, Some(price)) => {
            Some((pair.quote_currency.clone(), price * body.quantity))
        }
        (OrderType::Limit, OrderSide::Sell, _) | (OrderType::Market, OrderSide::Sell, _) => {
            Some((pair.base_currency.clone(), body.quantity))
        }
        _ => None, // market buy — no lock at placement
    };

    if let Some((currency, amount)) = &lock {
        wallet_service::repository::lock_balance(pool.get_ref(), user.id, currency, *amount)
            .await
            .map_err(|e| match e {
                wallet_service::repository::LockError::InsufficientBalance => {
                    ApiError::InsufficientBalance(format!(
                        "insufficient {currency} balance to place order"
                    ))
                }
                wallet_service::repository::LockError::Sqlx(err) => ApiError::Internal(err.into()),
            })?;
    }

    let order = repository::insert_order(
        pool.get_ref(),
        repository::NewOrder {
            user_id: user.id,
            pair: &pair_symbol,
            side: side_str,
            order_type: type_str,
            price: body.price,
            quantity: body.quantity,
        },
    )
    .await
    .map_err(|e| ApiError::Internal(e.into()))?;

    tracing::info!(order_id = %order.id, pair = %pair_symbol, side = %side_str, "order placed");

    Ok(HttpResponse::Created().json(PlaceOrderResponse {
        id: order.id,
        pair: order.pair,
        side: order.side,
        order_type: order.order_type,
        status: order.status,
        price: order.price,
        quantity: order.quantity,
        filled_qty: order.filled_qty,
        created_at: order.created_at,
    }))
}

// ---------------------------------------------------------------------------
// GET /orders
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ListOrdersQuery {
    pub pair: Option<String>,
    pub status: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[get("/orders")]
#[tracing::instrument(name = "order.list", skip_all, fields(user_id = %user.id))]
pub async fn list_orders(
    pool: web::Data<PgPool>,
    user: AuthUser,
    query: web::Query<ListOrdersQuery>,
) -> Result<HttpResponse, ApiError> {
    let filter = OrderFilter {
        pair: query.pair.as_deref().map(str::to_uppercase),
        status: query.status.clone(),
        limit: query.limit.unwrap_or(20).min(100),
        offset: query.offset.unwrap_or(0).max(0),
    };

    let orders = repository::list_orders(pool.get_ref(), user.id, filter)
        .await
        .map_err(|e| ApiError::Internal(e.into()))?;

    let body: Vec<OrderResponse> = orders.into_iter().map(OrderResponse::from).collect();
    Ok(HttpResponse::Ok().json(body))
}

// ---------------------------------------------------------------------------
// GET /orders/:id
// ---------------------------------------------------------------------------

#[get("/orders/{id}")]
#[tracing::instrument(name = "order.get", skip_all, fields(user_id = %user.id, order_id = %path))]
pub async fn get_order(
    pool: web::Data<PgPool>,
    user: AuthUser,
    path: web::Path<Uuid>,
) -> Result<HttpResponse, ApiError> {
    let order_id = path.into_inner();
    let order = repository::find_order(pool.get_ref(), order_id, user.id)
        .await
        .map_err(|e| ApiError::Internal(e.into()))?
        .ok_or_else(|| ApiError::NotFound(format!("order '{order_id}' not found")))?;

    Ok(HttpResponse::Ok().json(OrderResponse::from(order)))
}

// ---------------------------------------------------------------------------
// DELETE /orders/:id
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct CancelOrderResponse {
    pub id: Uuid,
    pub status: String,
}

#[delete("/orders/{id}")]
#[tracing::instrument(name = "order.cancel", skip_all, fields(user_id = %user.id, order_id = %path))]
pub async fn cancel_order(
    pool: web::Data<PgPool>,
    user: AuthUser,
    path: web::Path<Uuid>,
) -> Result<HttpResponse, ApiError> {
    let order_id = path.into_inner();

    let order = repository::cancel_order(pool.get_ref(), order_id, user.id)
        .await
        .map_err(|e| ApiError::Internal(e.into()))?
        .ok_or_else(|| {
            ApiError::NotFound(format!(
                "order '{order_id}' not found or cannot be cancelled"
            ))
        })?;

    // Unlock the balance that was locked when the order was placed.
    // For partially_filled orders only the remaining (unfilled) portion was still locked.
    let remaining = order.quantity - order.filled_qty;
    let pair = repository::find_pair(pool.get_ref(), &order.pair)
        .await
        .map_err(|e| ApiError::Internal(e.into()))?;

    if let Some(pair) = pair {
        let unlock = match (order.side.as_str(), order.order_type.as_str(), order.price) {
            ("buy", "limit", Some(price)) => Some((pair.quote_currency, price * remaining)),
            ("sell", _, _) => Some((pair.base_currency, remaining)),
            _ => None, // market buy had no lock at placement
        };

        if let Some((currency, amount)) = unlock {
            wallet_service::repository::unlock_balance(pool.get_ref(), user.id, &currency, amount)
                .await
                .map_err(|e| ApiError::Internal(e.into()))?;
        }
    }

    tracing::info!(order_id = %order.id, "order cancelled");

    Ok(HttpResponse::Ok().json(CancelOrderResponse {
        id: order.id,
        status: order.status,
    }))
}
