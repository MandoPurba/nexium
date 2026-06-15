mod error;
mod jwt;
mod password;
mod repository;
mod routes;

use actix_web::{App, HttpServer, web};
use nexium_config::AppConfig;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cfg = AppConfig::load(env!("CARGO_PKG_NAME"))?;
    nexium_telemetry::init(&cfg.telemetry, cfg.environment)?;

    let pool = nexium_db::pg_pool(&cfg.database).await?;
    let issuer = jwt::JwtIssuer::new(cfg.auth.jwt_secret.expose(), cfg.auth.jwt_expiry_secs);

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
            .service(routes::register)
            .service(routes::login)
    })
    .bind((host.as_str(), port))?
    .run()
    .await?;

    Ok(())
}
