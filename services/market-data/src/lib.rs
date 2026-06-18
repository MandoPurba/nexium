pub mod repository;
pub mod routes;

use actix_web::{App, HttpServer, web};
use nexium_config::AppConfig;
use routes::TimescalePool;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

#[derive(OpenApi)]
#[openapi(
    paths(
        routes::get_ohlcv,
        routes::get_orderbook,
        routes::get_trades,
    ),
    components(schemas(
        routes::OhlcvResponse,
        routes::OrderBookResponse,
        routes::TradeResponse,
        routes::ErrorResponse,
    )),
    tags(
        (name = "Market Data", description = "OHLCV candles, orderbook snapshots, and trade history")
    ),
)]
pub struct ApiDoc;

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(routes::get_ohlcv)
        .service(routes::get_orderbook)
        .service(routes::get_trades);
}

pub async fn run() -> anyhow::Result<()> {
    let cfg = AppConfig::load(env!("CARGO_PKG_NAME"))?;
    nexium_telemetry::init(&cfg.telemetry, cfg.environment)?;

    let pg_pool = nexium_db::pg_pool(&cfg.database).await?;
    let ts_pool = nexium_db::timescale_pool(&cfg.timescale).await?;

    let host = cfg.server.host.clone();
    let port = cfg.server.port;

    tracing::info!(host = %host, port = port, "market-data-service listening");

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(pg_pool.clone()))
            .app_data(web::Data::new(TimescalePool(ts_pool.clone())))
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
