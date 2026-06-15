//! Tracing subscriber initialization for Nexium services.
//!
//! - `Local`            → pretty, human-readable, ANSI colors
//! - `Development/Production` → JSON, structured, one event per line
//!
//! Filter precedence: `RUST_LOG` env var, then `telemetry.log_level` from config,
//! then `"info"` as a last resort.
//!
//! OpenTelemetry / OTLP export is intentionally deferred to Sprint 7.

use nexium_config::{Environment, TelemetryConfig};
use thiserror::Error;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Debug, Error)]
pub enum TelemetryError {
    #[error("invalid log filter directives: {0}")]
    Filter(#[from] tracing_subscriber::filter::ParseError),

    #[error("global tracing subscriber already initialized: {0}")]
    AlreadyInitialized(#[from] tracing_subscriber::util::TryInitError),
}

/// Install a global tracing subscriber for the calling service.
///
/// Idempotent within a process: the global subscriber may only be set once,
/// so a second call returns [`TelemetryError::AlreadyInitialized`].
pub fn init(cfg: &TelemetryConfig, env: Environment) -> Result<(), TelemetryError> {
    let directives = std::env::var("RUST_LOG").unwrap_or_else(|_| cfg.log_level.clone());
    let filter = EnvFilter::try_new(&directives).or_else(|_| EnvFilter::try_new("info"))?;

    let registry = tracing_subscriber::registry().with(filter);

    match env {
        Environment::Local => {
            registry
                .with(
                    fmt::layer()
                        .pretty()
                        .with_target(true)
                        .with_thread_ids(false)
                        .with_file(false)
                        .with_line_number(false),
                )
                .try_init()?;
        }
        Environment::Development | Environment::Production => {
            registry
                .with(
                    fmt::layer()
                        .json()
                        .with_current_span(true)
                        .with_span_list(false)
                        .with_target(true),
                )
                .try_init()?;
        }
    }

    tracing::info!(
        service = %cfg.service_name,
        env = %env,
        filter = %directives,
        "telemetry initialized"
    );

    Ok(())
}
