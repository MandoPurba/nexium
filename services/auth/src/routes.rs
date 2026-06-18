//! HTTP handlers for the auth service.

use actix_web::{HttpResponse, get, post, web};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use utoipa::ToSchema;
use uuid::Uuid;
use validator::Validate;

use nexium_core::error::ApiError;
use nexium_core::extractors::AuthUser;
use nexium_core::jwt::JwtIssuer;

use crate::password;
use crate::repository::{self, NewUser, RepoError};

// ---- POST /auth/register --------------------------------------------------

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct RegisterRequest {
    #[validate(email)]
    #[schema(example = "alice@example.com")]
    pub email: String,

    #[validate(length(min = 8, max = 256))]
    #[schema(example = "s3cureP@ss")]
    pub password: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UserResponse {
    pub id: Uuid,
    #[schema(example = "alice@example.com")]
    pub email: String,
    #[schema(example = "pending")]
    pub status: String,
    pub created_at: DateTime<Utc>,
}

#[utoipa::path(
    post,
    path = "/auth/register",
    tag = "Auth",
    request_body = RegisterRequest,
    responses(
        (status = 201, description = "User registered", body = UserResponse),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 409, description = "Email already registered", body = ErrorResponse),
    )
)]
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

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct LoginRequest {
    #[validate(email)]
    #[schema(example = "alice@example.com")]
    pub email: String,

    #[validate(length(min = 1, max = 256))]
    #[schema(example = "s3cureP@ss")]
    pub password: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct LoginResponse {
    #[schema(example = "eyJhbGciOiJIUzI1NiIs...")]
    pub access_token: String,
    #[schema(example = "Bearer")]
    pub token_type: &'static str,
    #[schema(example = 3600)]
    pub expires_in: i64,
}

#[utoipa::path(
    post,
    path = "/auth/login",
    tag = "Auth",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Login successful", body = LoginResponse),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Invalid credentials", body = ErrorResponse),
    )
)]
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

#[derive(Debug, Serialize, ToSchema)]
pub struct MeResponse {
    pub id: Uuid,
    #[schema(example = "alice@example.com")]
    pub email: String,
    #[schema(example = "active")]
    pub status: String,
    #[schema(example = "basic")]
    pub kyc_level: String,
}

#[derive(Debug, sqlx::FromRow)]
struct MeRow {
    id: Uuid,
    email: String,
    status: String,
    kyc_level: String,
}

#[utoipa::path(
    get,
    path = "/auth/me",
    tag = "Auth",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Current user profile", body = MeResponse),
        (status = 401, description = "Not authenticated", body = ErrorResponse),
    )
)]
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

// ---- POST /auth/api-keys ---------------------------------------------------

#[derive(Debug, Deserialize, Validate, ToSchema)]
pub struct CreateApiKeyRequest {
    #[schema(example = json!(["read", "trade"]))]
    pub permissions: Vec<String>,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ApiKeyResponse {
    pub id: Uuid,
    #[schema(example = "nex_live_a1b2c3d4e5f6...")]
    pub key: String,
    #[schema(example = json!(["read", "trade"]))]
    pub permissions: Vec<String>,
    pub expires_at: Option<DateTime<Utc>>,
}

fn generate_api_key() -> String {
    let random_bytes: [u8; 24] = rand::random();
    format!("nex_live_{}", hex::encode(random_bytes))
}

fn hash_api_key(key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    hex::encode(hasher.finalize())
}

#[utoipa::path(
    post,
    path = "/auth/api-keys",
    tag = "Auth",
    security(("bearer_auth" = [])),
    request_body = CreateApiKeyRequest,
    responses(
        (status = 201, description = "API key created (key shown once)", body = ApiKeyResponse),
        (status = 400, description = "Invalid permissions", body = ErrorResponse),
        (status = 401, description = "Not authenticated", body = ErrorResponse),
    )
)]
#[post("/auth/api-keys")]
#[tracing::instrument(name = "auth.create_api_key", skip_all, fields(user_id = %user.id))]
pub async fn create_api_key(
    pool: web::Data<PgPool>,
    user: AuthUser,
    body: web::Json<CreateApiKeyRequest>,
) -> Result<HttpResponse, ApiError> {
    let body = body.into_inner();

    let valid_perms = ["read", "trade", "withdraw"];
    for p in &body.permissions {
        if !valid_perms.contains(&p.as_str()) {
            return Err(ApiError::Validation(validator::ValidationErrors::new()));
        }
    }

    let raw_key = generate_api_key();
    let key_hash = hash_api_key(&raw_key);

    let perms: Vec<&str> = body.permissions.iter().map(|s| s.as_str()).collect();

    let id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO auth.api_keys (user_id, key_hash, permissions, expires_at)
        VALUES ($1, $2, $3, $4)
        RETURNING id
        "#,
    )
    .bind(user.id)
    .bind(&key_hash)
    .bind(&perms)
    .bind(body.expires_at)
    .fetch_one(pool.get_ref())
    .await
    .map_err(|e| ApiError::Internal(e.into()))?;

    tracing::info!(api_key_id = %id, "api key created");

    Ok(HttpResponse::Created().json(ApiKeyResponse {
        id,
        key: raw_key,
        permissions: body.permissions,
        expires_at: body.expires_at,
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
