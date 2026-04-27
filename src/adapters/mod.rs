//! Agent client adapter boundary.
//!
//! Adapters expose a generic contract to the Control Plane. Concrete clients
//! (pi, Claude Code, Codex, etc.) can implement this contract later without
//! leaking client-specific fields into domain events or External API views.

use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::runtime::AgentInput;

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterCapabilities {
    pub accept_task: bool,
    pub report_turn_started: bool,
    pub report_turn_finished: bool,
    pub interrupt: bool,
    pub stream_output: bool,
    pub heartbeat: bool,
    pub artifact_sources: bool,
}

impl AdapterCapabilities {
    pub fn generic_default() -> Self {
        Self {
            accept_task: true,
            report_turn_started: true,
            report_turn_finished: true,
            interrupt: false,
            stream_output: false,
            heartbeat: false,
            artifact_sources: false,
        }
    }

    pub fn pi_m0_default() -> Self {
        Self {
            accept_task: true,
            report_turn_started: true,
            report_turn_finished: true,
            interrupt: false,
            stream_output: true,
            heartbeat: false,
            artifact_sources: true,
        }
    }
}

pub trait AgentInputSink {
    fn accept_input(&self, input: AgentInput) -> crate::error::Result<()>;
}

pub trait AgentEventSource {
    fn capabilities(&self) -> AdapterCapabilities;
}

pub trait ArtifactSourceProvider {
    fn artifact_sources_enabled(&self) -> bool;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArtifactRegistration {
    pub artifact_id: String,
    pub session_id: String,
    pub turn_id: Option<String>,
    pub kind: String,
    pub name: String,
    pub source_ref: String,
    pub size_bytes: Option<i64>,
    pub metadata: Value,
}

#[derive(Debug, Default, Clone)]
pub struct GenericTestAdapter;

impl GenericTestAdapter {
    pub fn clear_recorded_inputs() {
        recorded_inputs()
            .lock()
            .expect("recorded inputs lock")
            .clear();
    }

    pub fn recorded_inputs() -> Vec<AgentInput> {
        recorded_inputs()
            .lock()
            .expect("recorded inputs lock")
            .clone()
    }
}

impl AgentInputSink for GenericTestAdapter {
    fn accept_input(&self, input: AgentInput) -> crate::error::Result<()> {
        recorded_inputs()
            .lock()
            .expect("recorded inputs lock")
            .push(input);
        Ok(())
    }
}

impl AgentEventSource for GenericTestAdapter {
    fn capabilities(&self) -> AdapterCapabilities {
        AdapterCapabilities::generic_default()
    }
}

impl ArtifactSourceProvider for GenericTestAdapter {
    fn artifact_sources_enabled(&self) -> bool {
        self.capabilities().artifact_sources
    }
}

fn recorded_inputs() -> &'static Mutex<Vec<AgentInput>> {
    static RECORDED_INPUTS: OnceLock<Mutex<Vec<AgentInput>>> = OnceLock::new();
    RECORDED_INPUTS.get_or_init(|| Mutex::new(Vec::new()))
}
