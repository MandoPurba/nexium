//! API error type that serializes to the project's error envelope:
//!
//! ```json
//! { "code": "VALIDATION_ERROR", "message": "...", "details": { ... } }
//! ```

use actix_web::{HttpResponse, ResponseError, http::StatusCode};
use serde_json::{Value, json};
use thiserror::Error;
use validator::ValidationErrors;

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("validation failed")]
    Validation(#[from] ValidationErrors),

    #[error("{0}")]
    Conflict(String),

    #[error("invalid credentials")]
    Unauthorized,

    #[error("{0}")]
    Forbidden(String),

    #[error("{0}")]
    NotFound(String),

    #[error("{0}")]
    InsufficientBalance(String),

    #[error("{0}")]
    PairInactive(String),

    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl ApiError {
    fn code(&self) -> &'static str {
        match self {
            Self::Validation(_) => "VALIDATION_ERROR",
            Self::Conflict(_) => "CONFLICT",
            Self::Unauthorized => "UNAUTHORIZED",
            Self::Forbidden(_) => "FORBIDDEN",
            Self::NotFound(_) => "NOT_FOUND",
            Self::InsufficientBalance(_) => "INSUFFICIENT_BALANCE",
            Self::PairInactive(_) => "PAIR_INACTIVE",
            Self::Internal(_) => "INTERNAL_ERROR",
        }
    }

    fn message(&self) -> String {
        match self {
            Self::Validation(_) => "request validation failed".to_string(),
            Self::Conflict(m) => m.clone(),
            Self::Unauthorized => "invalid credentials".to_string(),
            Self::Forbidden(m) => m.clone(),
            Self::NotFound(m) => m.clone(),
            Self::InsufficientBalance(m) => m.clone(),
            Self::PairInactive(m) => m.clone(),
            Self::Internal(_) => "internal server error".to_string(),
        }
    }

    fn details(&self) -> Option<Value> {
        match self {
            Self::Validation(e) => serde_json::to_value(e).ok(),
            _ => None,
        }
    }
}

impl ResponseError for ApiError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::Validation(_) => StatusCode::BAD_REQUEST,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::Forbidden(_) => StatusCode::FORBIDDEN,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::InsufficientBalance(_) | Self::PairInactive(_) => {
                StatusCode::UNPROCESSABLE_ENTITY
            }
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn error_response(&self) -> HttpResponse {
        if let Self::Internal(err) = self {
            tracing::error!(error = %err, "internal error");
        }

        let mut body = json!({
            "code": self.code(),
            "message": self.message(),
        });
        if let Some(d) = self.details() {
            body["details"] = d;
        }

        HttpResponse::build(self.status_code()).json(body)
    }
}
