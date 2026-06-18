use nexium_config::{Environment, TelemetryConfig};
use opentelemetry_otlp::WithExportConfig;
use thiserror::Error;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Debug, Error)]
pub enum TelemetryError {
    #[error("invalid log filter directives: {0}")]
    Filter(#[from] tracing_subscriber::filter::ParseError),

    #[error("global tracing subscriber already initialized: {0}")]
    AlreadyInitialized(#[from] tracing_subscriber::util::TryInitError),

    #[error("opentelemetry setup failed: {0}")]
    OpenTelemetry(String),
}

pub fn init(cfg: &TelemetryConfig, env: Environment) -> Result<(), TelemetryError> {
    let directives = std::env::var("RUST_LOG").unwrap_or_else(|_| cfg.log_level.clone());
    let filter = EnvFilter::try_new(&directives).or_else(|_| EnvFilter::try_new("info"))?;

    let tracer = if let Some(ref endpoint) = cfg.otlp_endpoint {
        match build_tracer(endpoint, &cfg.service_name) {
            Ok(t) => Some(t),
            Err(err) => {
                eprintln!("WARN: failed to init opentelemetry, continuing without traces: {err}");
                None
            }
        }
    } else {
        None
    };

    let otel_layer = tracer.map(|t| tracing_opentelemetry::layer().with_tracer(t));

    match env {
        Environment::Local => {
            tracing_subscriber::registry()
                .with(filter)
                .with(otel_layer)
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
            tracing_subscriber::registry()
                .with(filter)
                .with(otel_layer)
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

fn build_tracer(
    endpoint: &str,
    service_name: &str,
) -> Result<opentelemetry_sdk::trace::Tracer, Box<dyn std::error::Error>> {
    use opentelemetry::KeyValue;
    use opentelemetry::trace::TracerProvider as _;
    use opentelemetry_otlp::SpanExporter;
    use opentelemetry_sdk::Resource;
    use opentelemetry_sdk::runtime::Tokio;
    use opentelemetry_sdk::trace::{Sampler, TracerProvider};

    let exporter = SpanExporter::builder()
        .with_tonic()
        .with_endpoint(endpoint)
        .build()?;

    let provider = TracerProvider::builder()
        .with_batch_exporter(exporter, Tokio)
        .with_sampler(Sampler::AlwaysOn)
        .with_resource(Resource::new(vec![KeyValue::new(
            "service.name",
            service_name.to_string(),
        )]))
        .build();

    let tracer = provider.tracer(service_name.to_string());
    opentelemetry::global::set_tracer_provider(provider);

    Ok(tracer)
}

pub fn shutdown() {
    opentelemetry::global::shutdown_tracer_provider();
}
