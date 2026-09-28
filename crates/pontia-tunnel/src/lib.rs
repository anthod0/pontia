mod client;
pub mod protocol;

pub use client::RemoteClient;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Protocol(&'static str),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("random number generation failed: {0}")]
    Random(#[from] getrandom::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
