pub mod domain;
pub mod error;
pub mod ids;
pub mod time;

pub use error::{Error, Result};

pub mod client_capabilities;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionMode {
    #[default]
    Default,
    Yolo,
    Restricted,
}

impl SessionMode {
    pub fn bypasses_approvals(self) -> bool {
        matches!(self, Self::Yolo | Self::Restricted)
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LaunchMode {
    #[default]
    Foreground,
    Background,
}
