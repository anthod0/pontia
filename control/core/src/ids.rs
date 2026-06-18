use std::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ExternalId(String);

impl ExternalId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ExternalId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

pub fn new_session_id() -> ExternalId {
    prefixed_id("sess")
}

pub fn new_turn_id() -> ExternalId {
    prefixed_id("turn")
}

pub fn new_event_id() -> ExternalId {
    prefixed_id("evt")
}

pub fn new_runtime_instance_id() -> ExternalId {
    prefixed_id("rtinst")
}

pub fn new_artifact_id() -> ExternalId {
    prefixed_id("art")
}

pub fn new_workspace_id() -> ExternalId {
    prefixed_id("wks")
}

pub fn new_task_id() -> ExternalId {
    prefixed_id("task")
}

pub fn new_message_id() -> ExternalId {
    prefixed_id("msg")
}

pub fn new_agent_binding_id() -> ExternalId {
    prefixed_id("bind")
}

fn prefixed_id(prefix: &str) -> ExternalId {
    ExternalId(format!("{prefix}_{}", Uuid::now_v7()))
}
