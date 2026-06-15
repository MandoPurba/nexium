//! HTTP handlers for the auth service.

use actix_web::{HttpResponse, get, post, web};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;
use validator::Validate;

use nexium_core::error::ApiError;
use nexium_core::extractors::AuthUser;
use nexium_core::jwt::JwtIssuer;

use crate::password;
use crate::repository::{self, NewUser, RepoError};

// ---- POST /auth/register --------------------------------------------------

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

    // Direct fn call into wallet-service for now; becomes a NATS event once
    // messaging lands (Sprint 5+).
    wallet_service::repository::create_default_wallets(pool.get_ref(), user.id)
        .await
        .map_err(|e| ApiError::Internal(e.into()))?;

    tracing::info!(user_id = %user.id, "user registered");

    Ok(HttpResponse::Created().json(UserResponse {
        id: user.id,
        email: user.email,
        status: user.status,
        created_at: user.created_at,
    }))
}

// ---- POST /auth/login -----------------------------------------------------

#[derive(Debug, Deserialize, Validate)]
pub struct LoginRequest {
    #[validate(email)]
    pub email: String,

    // No min length here — we accept whatever the user typed and rely on
    // the verify step to reject. Cap at 256 to prevent argon2 DoS.
    #[validate(length(min = 1, max = 256))]
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub access_token: String,
    pub token_type: &'static str,
    pub expires_in: i64,
}

#[post("/auth/login")]
#[tracing::instrument(name = "auth.login", skip_all, fields(email = %body.email))]
pub async fn login(
    pool: web::Data<PgPool>,
    issuer: web::Data<JwtIssuer>,
    body: web::Json<LoginRequest>,
) -> Result<HttpResponse, ApiError> {
    let body = body.into_inner();
    body.validate()?;

    let email = body.email.trim().to_lowercase();
    let password = body.password;

    let user_opt = repository::find_by_email(pool.get_ref(), &email)
        .await
        .map_err(|e| ApiError::Internal(e.into()))?;

    // Verify in web::block (CPU-bound). When the user doesn't exist we still
    // run argon2 against a dummy hash so an attacker can't tell unknown email
    // from wrong password by timing.
    let (valid, user_id) = web::block(
        move || -> Result<(bool, Option<Uuid>), argon2::password_hash::Error> {
            match user_opt {
                Some(u) => Ok((password::verify(&password, &u.password_hash)?, Some(u.id))),
                None => {
                    let _ = password::verify(&password, password::dummy_hash())?;
                    Ok((false, None))
                }
            }
        },
    )
    .await
    .map_err(|e| ApiError::Internal(anyhow::anyhow!("verify join error: {e}")))?
    .map_err(|e| ApiError::Internal(anyhow::anyhow!("argon2 verify failed: {e}")))?;

    if !valid {
        tracing::info!(%email, "login rejected");
        return Err(ApiError::Unauthorized);
    }
    let user_id = user_id.expect("valid implies user found");

    let (token, expires_in) = issuer
        .issue(user_id)
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("jwt encode failed: {e}")))?;

    tracing::info!(user_id = %user_id, "login succeeded");

    Ok(HttpResponse::Ok().json(LoginResponse {
        access_token: token,
        token_type: "Bearer",
        expires_in,
    }))
}

// ---- GET /auth/me ---------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct MeResponse {
    pub id: Uuid,
    pub email: String,
    pub status: String,
    pub kyc_level: String,
}

#[derive(Debug, sqlx::FromRow)]
struct MeRow {
    id: Uuid,
    email: String,
    status: String,
    kyc_level: String,
}

#[get("/auth/me")]
#[tracing::instrument(name = "auth.me", skip_all, fields(user_id = %user.id))]
pub async fn me(pool: web::Data<PgPool>, user: AuthUser) -> Result<HttpResponse, ApiError> {
    let row = sqlx::query_as::<_, MeRow>(
        r#"
        SELECT
            u.id,
            u.email,
            u.status::text AS status,
            COALESCE(
                (
                    SELECT k.level::text
                    FROM auth.kyc k
                    WHERE k.user_id = u.id
                    ORDER BY k.created_at DESC
                    LIMIT 1
                ),
                'none'
            ) AS kyc_level
        FROM auth.users u
        WHERE u.id = $1
        "#,
    )
    .bind(user.id)
    .fetch_optional(pool.get_ref())
    .await
    .map_err(|e| ApiError::Internal(e.into()))?
    .ok_or(ApiError::Unauthorized)?;

    Ok(HttpResponse::Ok().json(MeResponse {
        id: row.id,
        email: row.email,
        status: row.status,
        kyc_level: row.kyc_level,
    }))
}
