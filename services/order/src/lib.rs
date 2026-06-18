//! Order service — library entry point.
//!
//! The binary (`src/main.rs`) is a thin wrapper around [`run`]. Tests import
//! [`configure`] to mount the same routes on an in-process Actix `App`.

pub mod repository;
pub mod routes;
pub mod settlement;
pub mod snapshot;

use actix_web::{App, HttpServer, web};
use nexium_config::AppConfig;
use nexium_core::jwt::JwtIssuer;
use nexium_core::metrics::metrics_handler;
use nexium_core::middleware::JwtAuth;
use nexium_core::rate_limit::ip_rate_limiter;
use nexium_matching_engine::{Engine, EngineCommand};
use sqlx::PgPool;
use tokio::sync::mpsc;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

const ENGINE_CMD_BUF: usize = 1024;
const ENGINE_EVT_BUF: usize = 1024;

pub type EngineSender = mpsc::Sender<EngineCommand>;

pub fn spawn_engine(
    pool: PgPool,
    ts_pool: Option<PgPool>,
    nats: Option<async_nats::Client>,
) -> EngineSender {
    let (cmd_tx, cmd_rx) = mpsc::channel(ENGINE_CMD_BUF);
    let (evt_tx, evt_rx) = mpsc::channel(ENGINE_EVT_BUF);

    let engine = Engine::new();
    tokio::spawn(engine.run(cmd_rx, evt_tx));
    tokio::spawn(settlement::run(pool.clone(), ts_pool.clone(), nats, evt_rx));

    if let Some(ts) = ts_pool {
        tokio::spawn(snapshot::run(pool, ts, cmd_tx.clone()));
    }

    cmd_tx
}

#[derive(OpenApi)]
#[openapi(
    paths(
        routes::list_pairs,
        routes::place_order,
        routes::list_orders,
        routes::get_order,
        routes::cancel_order,
    ),
    components(schemas(
        routes::PairResponse,
        routes::PlaceOrderRequest,
        routes::PlaceOrderResponse,
        routes::OrderResponse,
        routes::CancelOrderResponse,
        routes::ErrorResponse,
    )),
    tags(
        (name = "Trading", description = "Order placement, management, and trading pairs")
    ),
    modifiers(&SecurityAddon)
)]
pub struct ApiDoc;

struct SecurityAddon;

impl utoipa::Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            "bearer_auth",
            utoipa::openapi::security::SecurityScheme::Http(utoipa::openapi::security::Http::new(
                utoipa::openapi::security::HttpAuthScheme::Bearer,
            )),
        );
    }
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(routes::list_pairs).service(
        web::scope("")
            .wrap(ip_rate_limiter(100))
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
    let ts_pool = match nexium_db::timescale_pool(&cfg.timescale).await {
        Ok(p) => Some(p),
        Err(err) => {
            tracing::warn!(error = %err, "timescaledb unavailable; OHLCV + snapshots disabled");
            None
        }
    };
    let nats = match async_nats::connect(&cfg.nats.url).await {
        Ok(nc) => {
            tracing::info!(url = %cfg.nats.url, "connected to nats");
            Some(nc)
        }
        Err(err) => {
            tracing::warn!(error = %err, "nats unavailable; realtime events disabled");
            None
        }
    };
    let issuer = JwtIssuer::new(cfg.auth.jwt_secret.expose(), cfg.auth.jwt_expiry_secs);
    let engine_tx = spawn_engine(pool.clone(), ts_pool, nats);

    let host = cfg.server.host.clone();
    let port = cfg.server.port;

    tracing::info!(host = %host, port = port, "order-service listening");

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(issuer.clone()))
            .app_data(web::Data::new(engine_tx.clone()))
            .service(metrics_handler)
            .service(
                SwaggerUi::new("/docs/{_:.*}").url("/api-docs/openapi.json", ApiDoc::openapi()),
            )
            .configure(configure)
    })
    .bind((host.as_str(), port))?
    .run()
    .await?;

    Ok(())
}
