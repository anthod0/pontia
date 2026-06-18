use serde::{Deserialize, Serialize};

use crate::error::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "TEXT", rename_all = "snake_case")]
pub enum SessionState {
    Created,
    Starting,
    Idle,
    Busy,
    Interrupted,
    Exited,
    Error,
}

impl SessionState {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Exited | Self::Error)
    }
}

impl std::fmt::Display for SessionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Created => "created",
            Self::Starting => "starting",
            Self::Idle => "idle",
            Self::Busy => "busy",
            Self::Interrupted => "interrupted",
            Self::Exited => "exited",
            Self::Error => "error",
        })
    }
}

impl std::str::FromStr for SessionState {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "created" => Ok(Self::Created),
            "starting" => Ok(Self::Starting),
            "idle" => Ok(Self::Idle),
            "busy" => Ok(Self::Busy),
            "interrupted" => Ok(Self::Interrupted),
            "exited" => Ok(Self::Exited),
            "error" => Ok(Self::Error),
            _ => Err(Error::Domain(format!("unknown session state: {value}"))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "TEXT", rename_all = "snake_case")]
pub enum TurnState {
    Queued,
    Running,
    Completed,
    Failed,
    Interrupted,
    Cancelled,
}

impl TurnState {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::Interrupted | Self::Cancelled
        )
    }

    pub fn is_active(self) -> bool {
        matches!(self, Self::Queued | Self::Running)
    }
}

impl std::fmt::Display for TurnState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Interrupted => "interrupted",
            Self::Cancelled => "cancelled",
        })
    }
}

impl std::str::FromStr for TurnState {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "queued" => Ok(Self::Queued),
            "running" => Ok(Self::Running),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "interrupted" => Ok(Self::Interrupted),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err(Error::Domain(format!("unknown turn state: {value}"))),
        }
    }
}
