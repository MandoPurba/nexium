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

    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl ApiError {
    fn code(&self) -> &'static str {
        match self {
            Self::Validation(_) => "VALIDATION_ERROR",
            Self::Conflict(_) => "CONFLICT",
            Self::Internal(_) => "INTERNAL_ERROR",
        }
    }

    fn message(&self) -> String {
        match self {
            Self::Validation(_) => "request validation failed".to_string(),
            Self::Conflict(m) => m.clone(),
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
