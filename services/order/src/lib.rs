//! Order service — library entry point.
//!
//! The binary (`src/main.rs`) is a thin wrapper around [`run`]. Tests import
//! [`configure`] to mount the same routes on an in-process Actix `App`.

pub mod repository;
pub mod routes;
pub mod settlement;

use actix_web::{App, HttpServer, web};
use nexium_config::AppConfig;
use nexium_core::jwt::JwtIssuer;
use nexium_core::middleware::JwtAuth;
use nexium_matching_engine::{Engine, EngineCommand};
use sqlx::PgPool;
use tokio::sync::mpsc;

/// Channel capacity for engine commands. Generous so HTTP handlers never
/// back-pressure on quick bursts.
const ENGINE_CMD_BUF: usize = 1024;
const ENGINE_EVT_BUF: usize = 1024;

/// Handle used by HTTP handlers to submit commands to the matching engine.
pub type EngineSender = mpsc::Sender<EngineCommand>;

/// Spawn the matching engine and settlement tasks. Returns the command-sender
/// half that HTTP handlers store in `app_data` to feed orders into the engine.
pub fn spawn_engine(pool: PgPool) -> EngineSender {
    let (cmd_tx, cmd_rx) = mpsc::channel(ENGINE_CMD_BUF);
    let (evt_tx, evt_rx) = mpsc::channel(ENGINE_EVT_BUF);

    let engine = Engine::new();
    tokio::spawn(engine.run(cmd_rx, evt_tx));
    tokio::spawn(settlement::run(pool, evt_rx));

    cmd_tx
}

/// Mount all order routes on `cfg`.
///
/// - `GET /pairs` is public (no auth required).
/// - All order routes are protected by [`JwtAuth`].
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(routes::list_pairs).service(
        web::scope("")
            .wrap(JwtAuth)
            .service(routes::place_order)
            .service(routes::list_orders)
            .service(routes::get_order)
            .service(routes::cancel_order),
    );
}

pub async fn run() -> anyhow::Result<()> {
    let cfg = AppConfig::load(env!("CARGO_PKG_NAME"))?;
    nexium_telemetry::init(&cfg.telemetry, cfg.environment)?;

    let pool = nexium_db::pg_pool(&cfg.database).await?;
    let issuer = JwtIssuer::new(cfg.auth.jwt_secret.expose(), cfg.auth.jwt_expiry_secs);
    let engine_tx = spawn_engine(pool.clone());

    let host = cfg.server.host.clone();
    let port = cfg.server.port;

    tracing::info!(host = %host, port = port, "order-service listening");

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(issuer.clone()))
            .app_data(web::Data::new(engine_tx.clone()))
            .configure(configure)
    })
    .bind((host.as_str(), port))?
    .run()
    .await?;

    Ok(())
}
