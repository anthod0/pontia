//! Runtime control boundary.
//!
//! The MVP generic runtime records a binding and immediately reports ready. This
//! module stays independent from HTTP transport details.

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{application::SessionCapabilities, error::Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeStartRequest {
    pub session_id: String,
    pub client_type: String,
    pub workspace: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeStartResult {
    pub runtime_kind: String,
    pub runtime_ref: String,
    pub capabilities: SessionCapabilities,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentInput {
    pub session_id: String,
    pub turn_id: String,
    pub input: String,
}

#[derive(Debug, Clone, Default)]
pub struct GenericRuntimeManager;

impl GenericRuntimeManager {
    pub fn start_session(&self, request: RuntimeStartRequest) -> RuntimeStartResult {
        RuntimeStartResult {
            runtime_kind: request.client_type,
            runtime_ref: format!("generic:{}", request.session_id),
            capabilities: SessionCapabilities {
                accept_task: true,
                interrupt: false,
                stream_output: false,
                heartbeat: false,
                artifact_sources: false,
            },
        }
    }

    pub fn submit_input(&self, _input: AgentInput) -> Result<()> {
        Ok(())
    }

    pub fn terminate_session(&self, _session_id: &str) -> Result<()> {
        Ok(())
    }

    pub fn restart_session(&self, request: RuntimeStartRequest) -> RuntimeStartResult {
        self.start_session(request)
    }
}

impl RuntimeStartResult {
    pub fn binding_metadata(&self) -> serde_json::Value {
        json!({ "capabilities": self.capabilities })
    }
}
