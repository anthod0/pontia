use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub use pontia_agent_clients::{AgentClientCapabilities, AgentInput};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeStartRequest {
    pub session_id: String,
    pub client_type: String,
    pub workspace: Option<String>,
    pub handle: Option<String>,
    pub role: Option<String>,
    pub agent_kind: Option<String>,
    pub start_command: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeStartResult {
    pub runtime_kind: String,
    pub runtime_handle: String,
    pub capabilities: AgentClientCapabilities,
    pub metadata: Value,
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
