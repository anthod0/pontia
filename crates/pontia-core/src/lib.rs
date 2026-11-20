pub mod domain;
pub mod error;
pub mod ids;
pub mod time;

pub use error::{Error, Result};

pub mod client_capabilities;
pub mod execution_policy;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LaunchMode {
    #[default]
    Foreground,
    Background,
}
