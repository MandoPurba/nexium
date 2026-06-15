//! Database pool construction and shared sqlx helpers.

use nexium_config::{DatabaseConfig, TimescaleConfig};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use std::time::Duration;

/// Construct a PostgreSQL connection pool from primary-DB config.
pub async fn pg_pool(cfg: &DatabaseConfig) -> Result<PgPool, sqlx::Error> {
    tracing::info!(
        max_connections = cfg.max_connections,
        min_connections = cfg.min_connections,
        "connecting to postgres"
    );
    PgPoolOptions::new()
        .max_connections(cfg.max_connections)
        .min_connections(cfg.min_connections)
        .acquire_timeout(Duration::from_secs(cfg.acquire_timeout_secs))
        .connect(&cfg.url)
        .await
}

/// Construct a TimescaleDB connection pool from market-DB config.
pub async fn timescale_pool(cfg: &TimescaleConfig) -> Result<PgPool, sqlx::Error> {
    tracing::info!(
        max_connections = cfg.max_connections,
        min_connections = cfg.min_connections,
        "connecting to timescaledb"
    );
    PgPoolOptions::new()
        .max_connections(cfg.max_connections)
        .min_connections(cfg.min_connections)
        .acquire_timeout(Duration::from_secs(cfg.acquire_timeout_secs))
        .connect(&cfg.url)
        .await
}
