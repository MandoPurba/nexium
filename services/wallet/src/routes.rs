//! HTTP handlers for the wallet service.

use actix_web::{HttpResponse, get, post, web};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use utoipa::ToSchema;
use uuid::Uuid;
use validator::{Validate, ValidationError};

use nexium_core::error::ApiError;
use nexium_core::extractors::AuthUser;

use crate::repository::{self, DepositError, WalletRecord};

// ---- shared response types -------------------------------------------------

#[derive(Debug, Serialize, ToSchema)]
pub struct WalletResponse {
    pub id: Uuid,
    #[schema(example = "USDT")]
    pub currency: String,
    #[schema(value_type = String, example = "1000.000000000000000000")]
    pub balance: Decimal,
    #[schema(value_type = String, example = "0.000000000000000000")]
    pub locked_balance: Decimal,
    #[schema(value_type = String, example = "1000.000000000000000000")]
    pub available: Decimal,
}

impl From<WalletRecord> for WalletResponse {
    fn from(w: WalletRecord) -> Self {
        let available = w.balance - w.locked_balance;
        Self {
            id: w.id,
            currency: w.currency,
            balance: w.balance,
            locked_balance: w.locked_balance,
            available,
        }
    }
}

// ---- GET /wallets -----------------------------------------------------------

#[utoipa::path(
    get,
    path = "/wallets",
    tag = "Wallet",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "List of user wallets", body = Vec<WalletResponse>),
        (status = 401, description = "Not authenticated", body = ErrorResponse),
    )
)]
#[get("/wallets")]
#[tracing::instrument(name = "wallet.list", skip_all, fields(user_id = %user.id))]
pub async fn list_wallets(
    pool: web::Data<PgPool>,
    user: AuthUser,
) -> Result<HttpResponse, ApiError> {
    let wallets = repository::find_by_user(pool.get_ref(), user.id)
        .await
        .map_err(|e| ApiError::Internal(e.into()))?;

    let resp: Vec<WalletResponse> = wallets.into_iter().map(WalletResponse::from).collect();
    Ok(HttpResponse::Ok().json(resp))
}

// ---- GET /wallets/{currency} -------------------------------------------------

#[utoipa::path(
    get,
    path = "/wallets/{currency}",
    tag = "Wallet",
    security(("bearer_auth" = [])),
    params(
        ("currency" = String, Path, description = "Currency code (e.g. BTC, USDT)")
    ),
    responses(
        (status = 200, description = "Wallet details", body = WalletResponse),
        (status = 401, description = "Not authenticated", body = ErrorResponse),
        (status = 404, description = "Wallet not found", body = ErrorResponse),
    )
)]
#[get("/wallets/{currency}")]
#[tracing::instrument(name = "wallet.get", skip_all, fields(user_id = %user.id))]
pub async fn get_wallet(
    pool: web::Data<PgPool>,
    user: AuthUser,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    let currency = path.into_inner().to_uppercase();

    let wallet = repository::find_by_currency(pool.get_ref(), user.id, &currency)
        .await
        .map_err(|e| ApiError::Internal(e.into()))?
        .ok_or_else(|| ApiError::NotFound(format!("wallet for currency {currency} not found")))?;

    Ok(HttpResponse::Ok().json(WalletResponse::from(wallet)))
}

// ---- POST /wallets/deposit ---------------------------------------------------

fn validate_positive_amount(amount: &Decimal) -> Result<(), ValidationError> {
    if *amount <= Decimal::ZERO {
        return Err(ValidationError::new("amount_must_be_positive"));
    }
    Ok(())
}

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct DepositRequest {
    #[validate(length(min = 1, max = 10))]
    #[schema(example = "USDT")]
    pub currency: String,

    #[validate(custom(function = "validate_positive_amount"))]
    #[schema(value_type = String, example = "100.50")]
    pub amount: Decimal,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DepositResponse {
    pub txn_id: Uuid,
    #[schema(example = "USDT")]
    pub currency: String,
    #[schema(value_type = String, example = "100.50")]
    pub amount: Decimal,
    #[schema(example = "confirmed")]
    pub status: String,
}

#[utoipa::path(
    post,
    path = "/wallets/deposit",
    tag = "Wallet",
    security(("bearer_auth" = [])),
    request_body = DepositRequest,
    responses(
        (status = 201, description = "Deposit confirmed", body = DepositResponse),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Not authenticated", body = ErrorResponse),
        (status = 404, description = "Currency not found", body = ErrorResponse),
    )
)]
#[post("/wallets/deposit")]
#[tracing::instrument(name = "wallet.deposit", skip_all, fields(user_id = %user.id, currency = %body.currency))]
pub async fn deposit(
    pool: web::Data<PgPool>,
    user: AuthUser,
    body: web::Json<DepositRequest>,
) -> Result<HttpResponse, ApiError> {
    let body = body.into_inner();
    body.validate()?;

    let currency = body.currency.trim().to_uppercase();

    let txn = repository::deposit(pool.get_ref(), user.id, &currency, body.amount)
        .await
        .map_err(|e| match e {
            DepositError::NotFound => {
                ApiError::NotFound(format!("wallet for currency {currency} not found"))
            }
            DepositError::Sqlx(err) => ApiError::Internal(err.into()),
        })?;

    tracing::info!(user_id = %user.id, %currency, amount = %txn.amount, "deposit confirmed");

    Ok(HttpResponse::Created().json(DepositResponse {
        txn_id: txn.id,
        currency,
        amount: txn.amount,
        status: txn.status,
    }))
}

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
