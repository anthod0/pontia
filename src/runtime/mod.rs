//! Runtime control boundary.
//!
//! The MVP generic runtime records a binding and immediately reports ready. This
//! module stays independent from HTTP transport details.

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    adapters::{AdapterCapabilities, AgentEventSource, AgentInputSink, GenericTestAdapter},
    application::SessionCapabilities,
    error::Result,
};

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
        }
    }
}

impl GenericRuntimeManager {
    pub fn start_session(&self, request: RuntimeStartRequest) -> RuntimeStartResult {
        let capabilities = match request.client_type.as_str() {
            "generic" => GenericTestAdapter.capabilities(),
            "pi" => AdapterCapabilities::pi_m0_default(),
            _ => AdapterCapabilities::default(),
        };
        RuntimeStartResult {
            runtime_kind: request.client_type.clone(),
            runtime_ref: format!("{}:{}", request.client_type, request.session_id),
            capabilities: capabilities.into(),
        }
    }

    pub fn submit_input(&self, input: AgentInput) -> Result<()> {
        GenericTestAdapter.accept_input(input)
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
