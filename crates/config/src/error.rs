use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("config build failed: {0}")]
    Config(#[from] config::ConfigError),

    #[error("unknown environment: '{0}' (expected: local | development | production)")]
    UnknownEnvironment(String),

    #[error("auth.jwt_secret is weak or unset; refusing to start in production")]
    WeakSecret,
}
