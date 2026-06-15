//! Auth service — library entry point.
//!
//! The binary (`src/main.rs`) is a thin wrapper around [`run`]. Tests import
//! [`configure`] to mount the same routes on an in-process Actix `App`.

pub mod error;
pub mod extractors;
pub mod jwt;
pub mod middleware;
pub mod password;
pub mod repository;
pub mod routes;

use actix_web::{App, HttpServer, web};
use nexium_config::AppConfig;

use crate::jwt::JwtIssuer;
use crate::middleware::JwtAuth;

/// Mount every auth route on `cfg`.
///
/// Public routes (`/auth/register`, `/auth/login`) sit at the top level.
/// Protected routes (`/auth/me`) live inside a scope wrapped with
/// [`JwtAuth`], which pulls the [`JwtIssuer`] from `app_data`.
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(routes::register)
        .service(routes::login)
        .service(web::scope("").wrap(JwtAuth).service(routes::me));
}

/// Bootstrap the auth-service process: load config, init telemetry, build
/// the DB pool, construct the JWT issuer, and serve until shutdown.
pub async fn run() -> anyhow::Result<()> {
    let cfg = AppConfig::load(env!("CARGO_PKG_NAME"))?;
    nexium_telemetry::init(&cfg.telemetry, cfg.environment)?;

    let pool = nexium_db::pg_pool(&cfg.database).await?;
    let issuer = JwtIssuer::new(cfg.auth.jwt_secret.expose(), cfg.auth.jwt_expiry_secs);

    let host = cfg.server.host.clone();
    let port = cfg.server.port;

    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        %host,
        port,
        "auth-service listening"
    );

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(issuer.clone()))
            .configure(configure)
    })
    .bind((host.as_str(), port))?
    .run()
    .await?;

    Ok(())
}
