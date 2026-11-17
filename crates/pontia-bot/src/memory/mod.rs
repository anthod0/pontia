mod commands;
mod keywords;
mod service;
mod storage;
mod types;

pub use commands::MemoryCommands;
pub use service::MemoryService;
pub use types::*;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    InvalidInput(String),
    #[error("Session is not authorized for this memory operation")]
    Forbidden,
    #[error("Memory not found in the permitted scope")]
    NotFound,
    #[error(transparent)]
    Storage(#[from] sqlx::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    TimeFormat(#[from] time::error::Format),
    #[error(transparent)]
    TimeParse(#[from] time::error::Parse),
}

pub type Result<T> = std::result::Result<T, Error>;
