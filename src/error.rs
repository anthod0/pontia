use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("invalid configuration for {key}: {message}")]
    InvalidConfig { key: &'static str, message: String },

    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("migration error: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("domain error: {0}")]
    Domain(String),

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}
