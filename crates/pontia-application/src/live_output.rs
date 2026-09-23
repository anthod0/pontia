use crate::client_contract::raw_transcripts::ManagedToolUse;
use serde::{Deserialize, Serialize};
use serde_json::Value;

mod service;
mod store;
mod validation;

pub use service::LiveOutputService;
pub(crate) use store::LiveOutputStore;
pub use store::LiveOutputSubscription;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum LiveOutputItem {
    AssistantText {
        item_id: String,
        text: String,
    },
    ToolCall {
        item_id: String,
        call_id: String,
        tool_name: String,
        arguments: Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        managed_tool_use: Option<ManagedToolUse>,
    },
}

impl LiveOutputItem {
    fn item_id(&self) -> &str {
        match self {
            Self::AssistantText { item_id, .. } | Self::ToolCall { item_id, .. } => item_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum LiveOutputUpdate {
    AssistantTextDelta {
        item_id: String,
        delta: String,
    },
    ToolCall {
        item_id: String,
        call_id: String,
        tool_name: String,
        arguments: Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        managed_tool_use: Option<ManagedToolUse>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LiveOutputIdentity {
    pub session_id: String,
    pub turn_id: String,
    pub stream_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveOutputProducer {
    pub identity: LiveOutputIdentity,
    pub runtime_instance_id: String,
}

#[derive(Debug, Clone)]
pub struct LiveOutputBatch {
    pub producer: LiveOutputProducer,
    pub first_sequence: u64,
    pub updates: Vec<LiveOutputUpdate>,
}

#[derive(Debug, Clone)]
pub struct LiveOutputSnapshotReplacement {
    pub producer: LiveOutputProducer,
    pub sequence: u64,
    pub items: Vec<LiveOutputItem>,
}

#[derive(Debug, Clone)]
pub struct LiveOutputClose {
    pub producer: LiveOutputProducer,
    pub sequence: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LiveOutputSnapshot {
    #[serde(flatten)]
    pub identity: LiveOutputIdentity,
    pub sequence: u64,
    pub items: Vec<LiveOutputItem>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveOutputCloseReason {
    ProducerClosed,
    Invalidated,
    Expired,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LiveOutputStreamEvent {
    Snapshot {
        #[serde(flatten)]
        snapshot: LiveOutputSnapshot,
    },
    Updates {
        #[serde(flatten)]
        identity: LiveOutputIdentity,
        first_sequence: u64,
        updates: Vec<LiveOutputUpdate>,
    },
    Closed {
        #[serde(flatten)]
        identity: LiveOutputIdentity,
        sequence: u64,
        reason: LiveOutputCloseReason,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveOutputPublishOutcome {
    Accepted {
        accepted_sequence: u64,
        duplicate: bool,
    },
    SnapshotRequired {
        accepted_sequence: u64,
    },
}
