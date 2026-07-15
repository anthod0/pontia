use std::path::PathBuf;

use pontia_core::Error;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentBindingResolveRequest {
    pub id: String,
    pub session_id: String,
    pub client_type: String,
    pub launch_cwd: PathBuf,
    pub client_session_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedAgentBinding {
    pub id: String,
    pub client_type: String,
    pub format: String,
    pub path: PathBuf,
    pub fingerprint: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimelineBoundaryCaptureKind {
    Head,
    Tail,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimelineBoundaryCaptureRequest {
    pub source: ResolvedAgentBinding,
    pub kind: TimelineBoundaryCaptureKind,
    pub native_entry_anchor: Option<String>,
    pub allow_missing_native_entry_anchor: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedTimelineBoundary {
    pub kind: TimelineBoundaryCaptureKind,
    pub cursor: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimelinePageRequest {
    pub session_id: String,
    pub source: ResolvedAgentBinding,
    pub before: Option<String>,
    pub after: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimelineItemDetailRequest {
    pub session_id: String,
    pub source: ResolvedAgentBinding,
    pub content_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnTimelineRange {
    pub turn_id: String,
    pub turn_index: i64,
    pub head_cursor: String,
    pub tail_cursor: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnTimelineReadRequest {
    pub source: ResolvedAgentBinding,
    pub ranges: Vec<TurnTimelineRange>,
}

#[derive(Debug)]
pub enum TurnTimelineReadError {
    InvalidRange { turn_id: String, message: String },
    Inner(Error),
}

impl std::fmt::Display for TurnTimelineReadError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRange { turn_id, message } => {
                write!(
                    formatter,
                    "invalid timeline range for Turn {turn_id}: {message}"
                )
            }
            Self::Inner(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for TurnTimelineReadError {}

impl From<Error> for TurnTimelineReadError {
    fn from(error: Error) -> Self {
        Self::Inner(error)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TurnTimelineItem {
    pub turn_id: String,
    #[serde(flatten)]
    pub item: TimelineItem,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimelineItem {
    pub item_id: String,
    pub kind: String,
    pub raw_kind: Option<String>,
    pub role: String,
    pub title: Option<String>,
    pub status: Option<String>,
    pub occurred_at: Option<String>,
    pub content_preview: String,
    pub content_ref: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub managed_tool_use: Option<ManagedToolUse>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManagedToolUse {
    pub tool_name: String,
    pub input: ManagedToolUseInput,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ManagedToolUseInput {
    Read {
        path: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        start_line: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        end_line: Option<u64>,
    },
    Edit {
        path: String,
        edits_count: u64,
    },
    Write {
        path: String,
    },
    Bash {
        command: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        timeout: Option<u64>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimelinePage {
    pub session_id: String,
    pub binding_id: String,
    pub items: Vec<TimelineItem>,
    pub head_cursor: Option<String>,
    pub tail_cursor: Option<String>,
    pub has_more: bool,
    pub source_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimelineItemDetailPage {
    pub binding_id: String,
    pub content_ref: String,
    pub content_type: String,
    pub text: String,
    pub size_bytes: usize,
}
