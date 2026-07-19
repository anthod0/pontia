use serde::{Deserialize, Serialize};

use crate::error::Error;

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum TurnTopology {
    #[default]
    Unknown,
    Root,
    Linked {
        parent_turn_id: String,
    },
}

impl TurnTopology {
    pub fn linked(parent_turn_id: impl Into<String>) -> Self {
        Self::Linked {
            parent_turn_id: parent_turn_id.into(),
        }
    }

    pub fn status(&self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Root => "root",
            Self::Linked { .. } => "linked",
        }
    }

    pub fn parent_turn_id(&self) -> Option<&str> {
        match self {
            Self::Linked { parent_turn_id } => Some(parent_turn_id),
            Self::Unknown | Self::Root => None,
        }
    }

    pub fn from_parts(status: &str, parent_turn_id: Option<String>) -> Result<Self, Error> {
        match (status, parent_turn_id) {
            ("unknown", None) => Ok(Self::Unknown),
            ("root", None) => Ok(Self::Root),
            ("linked", Some(parent_turn_id)) if !parent_turn_id.trim().is_empty() => {
                Ok(Self::Linked { parent_turn_id })
            }
            ("unknown" | "root" | "linked", _) => Err(Error::Domain(format!(
                "invalid Turn topology status/parent combination for {status}"
            ))),
            _ => Err(Error::Domain(format!(
                "unknown Turn topology status: {status}"
            ))),
        }
    }
}

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
    Abandoned,
}

impl TurnState {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::Interrupted | Self::Cancelled | Self::Abandoned
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
            Self::Abandoned => "abandoned",
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
            "abandoned" => Ok(Self::Abandoned),
            _ => Err(Error::Domain(format!("unknown turn state: {value}"))),
        }
    }
}
