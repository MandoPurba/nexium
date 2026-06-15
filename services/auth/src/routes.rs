//! HTTP handlers for the auth service.

use actix_web::{HttpResponse, post, web};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;
use validator::Validate;

use crate::error::ApiError;
use crate::password;
use crate::repository::{self, NewUser, RepoError};

#[derive(Debug, Deserialize, Validate)]
pub struct RegisterRequest {
    #[validate(email)]
    pub email: String,

    #[validate(length(min = 8, max = 256))]
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct UserResponse {
    pub id: Uuid,
    pub email: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

#[post("/auth/register")]
#[tracing::instrument(name = "auth.register", skip_all, fields(email = %body.email))]
pub async fn register(
    pool: web::Data<PgPool>,
    body: web::Json<RegisterRequest>,
) -> Result<HttpResponse, ApiError> {
    let body = body.into_inner();
    body.validate()?;

    let email = body.email.trim().to_lowercase();

    let hash = web::block(move || password::hash(&body.password))
        .await
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("hash join error: {e}")))?
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("argon2 hash failed: {e}")))?;

    let user = repository::insert_user(
        pool.get_ref(),
        NewUser {
            email: &email,
            password_hash: &hash,
        },
    )
    .await
    .map_err(|e| match e {
        RepoError::DuplicateEmail => ApiError::Conflict("email already registered".into()),
        RepoError::Sqlx(err) => ApiError::Internal(err.into()),
    })?;

    tracing::info!(user_id = %user.id, "user registered");

    Ok(HttpResponse::Created().json(UserResponse {
        id: user.id,
        email: user.email,
        status: user.status,
        created_at: user.created_at,
    }))
}
