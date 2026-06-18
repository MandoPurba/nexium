//! Wallet service — library entry point.
//!
//! The binary (`src/main.rs`) is a thin wrapper around [`run`]. Tests import
//! [`configure`] to mount the same routes on an in-process Actix `App`.

pub mod repository;
pub mod routes;

use actix_web::{App, HttpServer, web};
use nexium_config::AppConfig;
use nexium_core::jwt::JwtIssuer;
use nexium_core::middleware::JwtAuth;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

#[derive(OpenApi)]
#[openapi(
    paths(
        routes::list_wallets,
        routes::get_wallet,
        routes::deposit,
    ),
    components(schemas(
        routes::WalletResponse,
        routes::DepositRequest,
        routes::DepositResponse,
        routes::ErrorResponse,
    )),
    tags(
        (name = "Wallet", description = "Balance management and deposits")
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
    cfg.service(
        web::scope("")
            .wrap(JwtAuth)
            .service(routes::list_wallets)
            .service(routes::get_wallet)
            .service(routes::deposit),
    );
}

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
        "wallet-service listening"
    );

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(issuer.clone()))
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
