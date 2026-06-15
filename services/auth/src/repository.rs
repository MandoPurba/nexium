//! Persistence layer for `auth.users`.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug)]
pub struct NewUser<'a> {
    pub email: &'a str,
    pub password_hash: &'a str,
}

#[derive(Debug, sqlx::FromRow)]
pub struct UserRecord {
    pub id: Uuid,
    pub email: String,
    pub password_hash: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, thiserror::Error)]
pub enum RepoError {
    #[error("email already registered")]
    DuplicateEmail,

    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
}

/// Postgres SQLSTATE for `unique_violation`.
const UNIQUE_VIOLATION: &str = "23505";

pub async fn insert_user(pool: &PgPool, new: NewUser<'_>) -> Result<UserRecord, RepoError> {
    let row = sqlx::query_as::<_, UserRecord>(
        r#"
        INSERT INTO auth.users (email, password_hash)
        VALUES ($1, $2)
        RETURNING id, email, password_hash, status::text AS status, created_at
        "#,
    )
    .bind(new.email)
    .bind(new.password_hash)
    .fetch_one(pool)
    .await
    .map_err(|e| match &e {
        sqlx::Error::Database(db) if db.code().as_deref() == Some(UNIQUE_VIOLATION) => {
            RepoError::DuplicateEmail
        }
        _ => RepoError::Sqlx(e),
    })?;

    Ok(row)
}

pub async fn find_by_email(pool: &PgPool, email: &str) -> Result<Option<UserRecord>, sqlx::Error> {
    sqlx::query_as::<_, UserRecord>(
        r#"
        SELECT id, email, password_hash, status::text AS status, created_at
        FROM auth.users
        WHERE email = $1
        "#,
    )
    .bind(email)
    .fetch_optional(pool)
    .await
}
