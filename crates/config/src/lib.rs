//! Layered configuration loading for Nexium services.
//!
//! Sources (later wins):
//!   1. `config/default.toml`
//!   2. `config/{environment}.toml`  (e.g. `local`, `production`)
//!   3. Env vars with `NEXIUM__` prefix (`__` is the nesting separator)
//!   4. Standard env vars: `DATABASE_URL`, `TIMESCALE_URL`, `REDIS_URL`, `JWT_SECRET`
//!   5. The `service_name` argument to [`AppConfig::load`]
//!
//! `.env` is loaded best-effort before any of the above.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

mod error;
pub use error::ConfigError;

// --------------------------------------------------------------------------
// Config tree
// --------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub environment: Environment,
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub timescale: TimescaleConfig,
    pub redis: RedisConfig,
    pub telemetry: TelemetryConfig,
    pub auth: AuthConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,
    pub min_connections: u32,
    pub acquire_timeout_secs: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TimescaleConfig {
    pub url: String,
    pub max_connections: u32,
    pub min_connections: u32,
    pub acquire_timeout_secs: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RedisConfig {
    pub url: String,
    pub pool_size: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TelemetryConfig {
    pub log_level: String,
    pub service_name: String,
    #[serde(default)]
    pub otlp_endpoint: Option<String>,
}

#[derive(Clone, Deserialize)]
pub struct AuthConfig {
    pub jwt_secret: Secret<String>,
    pub jwt_expiry_secs: u64,
}

impl fmt::Debug for AuthConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AuthConfig")
            .field("jwt_secret", &self.jwt_secret)
            .field("jwt_expiry_secs", &self.jwt_expiry_secs)
            .finish()
    }
}

// --------------------------------------------------------------------------
// Environment
// --------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Environment {
    Local,
    Development,
    Production,
}

impl Environment {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Development => "development",
            Self::Production => "production",
        }
    }
}

impl FromStr for Environment {
    type Err = ConfigError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "local" => Ok(Self::Local),
            "dev" | "development" => Ok(Self::Development),
            "prod" | "production" => Ok(Self::Production),
            _ => Err(ConfigError::UnknownEnvironment(s.to_string())),
        }
    }
}

impl fmt::Display for Environment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// --------------------------------------------------------------------------
// Secret wrapper — masks value in Debug output
// --------------------------------------------------------------------------

#[derive(Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Secret<T>(T);

impl<T> Secret<T> {
    pub fn new(value: T) -> Self {
        Self(value)
    }

    pub fn expose(&self) -> &T {
        &self.0
    }
}

impl<T> fmt::Debug for Secret<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(***)")
    }
}

// --------------------------------------------------------------------------
// Loader
// --------------------------------------------------------------------------

const STANDARD_ENV_VARS: &[(&str, &str)] = &[
    ("DATABASE_URL", "database.url"),
    ("TIMESCALE_URL", "timescale.url"),
    ("REDIS_URL", "redis.url"),
    ("JWT_SECRET", "auth.jwt_secret"),
];

const WEAK_SECRETS: &[&str] = &["", "change-me-please", "changeme", "secret"];

impl AppConfig {
    /// Load configuration for `service_name` (typically `env!("CARGO_PKG_NAME")`).
    pub fn load(service_name: &str) -> Result<Self, ConfigError> {
        let _ = dotenvy::dotenv();

        let env: Environment = std::env::var("NEXIUM_ENV")
            .ok()
            .map(|s| s.parse::<Environment>())
            .transpose()?
            .unwrap_or(Environment::Local);

        let mut builder = config::Config::builder()
            .add_source(config::File::with_name("config/default").required(false))
            .add_source(config::File::with_name(&format!("config/{env}")).required(false))
            .add_source(
                config::Environment::with_prefix("NEXIUM")
                    .separator("__")
                    .try_parsing(true),
            );

        for (env_var, key) in STANDARD_ENV_VARS {
            if let Ok(val) = std::env::var(env_var) {
                builder = builder.set_override(*key, val)?;
            }
        }

        builder = builder
            .set_override("environment", env.as_str())?
            .set_override("telemetry.service_name", service_name)?;

        let cfg: AppConfig = builder.build()?.try_deserialize()?;
        cfg.validate()?;
        Ok(cfg)
    }

    fn validate(&self) -> Result<(), ConfigError> {
        if matches!(self.environment, Environment::Production)
            && WEAK_SECRETS.contains(&self.auth.jwt_secret.expose().as_str())
        {
            return Err(ConfigError::WeakSecret);
        }
        Ok(())
    }
}
