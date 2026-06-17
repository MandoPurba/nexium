pub mod ws;

use actix_web::{App, HttpServer, web};
use nexium_config::AppConfig;
use nexium_core::jwt::JwtIssuer;

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/ws", web::get().to(ws::ws_handler));
}

pub async fn run() -> anyhow::Result<()> {
    let cfg = AppConfig::load(env!("CARGO_PKG_NAME"))?;
    nexium_telemetry::init(&cfg.telemetry, cfg.environment)?;

    let nats = async_nats::connect(&cfg.nats.url).await?;
    tracing::info!(url = %cfg.nats.url, "connected to nats");

    let issuer = JwtIssuer::new(cfg.auth.jwt_secret.expose(), cfg.auth.jwt_expiry_secs);

    let host = cfg.server.host.clone();
    let port = cfg.server.port;

    tracing::info!(host = %host, port = port, "gateway listening");

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(nats.clone()))
            .app_data(web::Data::new(issuer.clone()))
            .configure(configure)
    })
    .bind((host.as_str(), port))?
    .run()
    .await?;

    Ok(())
}
