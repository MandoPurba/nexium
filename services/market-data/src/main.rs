use nexium_config::AppConfig;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cfg = AppConfig::load(env!("CARGO_PKG_NAME"))?;
    nexium_telemetry::init(&cfg.telemetry, cfg.environment)?;

    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        host = %cfg.server.host,
        port = cfg.server.port,
        "service starting"
    );

    Ok(())
}
