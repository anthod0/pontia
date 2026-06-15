use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{adapters::AdapterCapabilities, application::SessionCapabilities};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeStartRequest {
    pub session_id: String,
    pub client_type: String,
    pub workspace: Option<String>,
    pub handle: Option<String>,
    pub role: Option<String>,
    pub agent_kind: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeStartResult {
    pub runtime_kind: String,
    pub runtime_ref: String,
    pub capabilities: SessionCapabilities,
    pub metadata: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentInput {
    pub session_id: String,
    pub turn_id: String,
    pub input: String,
}

impl From<AdapterCapabilities> for SessionCapabilities {
    fn from(capabilities: AdapterCapabilities) -> Self {
        Self {
            accept_task: capabilities.accept_task,
            report_turn_started: capabilities.report_turn_started,
            report_turn_finished: capabilities.report_turn_finished,
            interrupt: capabilities.interrupt,
            stream_output: capabilities.stream_output,
            heartbeat: capabilities.heartbeat,
            artifact_sources: capabilities.artifact_sources,
            context_usage: capabilities.context_usage,
        }
    }
}

impl RuntimeStartResult {
    pub fn binding_metadata(&self) -> serde_json::Value {
        let mut metadata = self.metadata.clone();
        if let Some(object) = metadata.as_object_mut() {
            object.insert("capabilities".to_string(), json!(self.capabilities));
        }
        metadata
    }

    pub fn runtime_instance_id(&self) -> Option<&str> {
        self.metadata["runtime_instance_id"].as_str()
    }

    pub fn launch_cwd(&self) -> Option<&str> {
        self.metadata["launch_cwd"]
            .as_str()
            .or_else(|| self.metadata["workspace"].as_str())
    }

    pub fn last_seen_at(&self) -> Option<&str> {
        self.metadata["last_seen_at"]
            .as_str()
            .or_else(|| self.metadata["started_at"].as_str())
    }

    pub fn tmux_socket_path(&self) -> Option<&str> {
        self.metadata["tmux_socket_path"].as_str()
    }

    pub fn tmux_pane_id(&self) -> Option<&str> {
        self.metadata["tmux_pane_id"].as_str()
    }
}
