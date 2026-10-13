use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContextUsageCapability {
    #[default]
    Unsupported,
    Estimated,
    Exact,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentInput {
    pub session_id: String,
    pub dispatch_id: String,
    pub input: String,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentClientCapabilities {
    pub accept_task: bool,
    pub report_turn_started: bool,
    pub report_turn_finished: bool,
    pub interrupt: bool,
    pub stream_output: bool,
    pub heartbeat: bool,
    pub timeline: bool,
    pub topology: bool,
    pub branch_control: bool,
    pub list_models: bool,
    pub set_model: bool,
    pub manual_compaction: bool,
    pub compaction_events: bool,
    pub compaction_history: bool,
    pub context_usage: ContextUsageCapability,
}

impl AgentClientCapabilities {
    pub fn generic_default() -> Self {
        Self {
            accept_task: true,
            report_turn_started: true,
            report_turn_finished: true,
            interrupt: false,
            stream_output: false,
            heartbeat: false,
            timeline: false,
            topology: false,
            branch_control: false,
            list_models: false,
            set_model: false,
            manual_compaction: false,
            compaction_events: false,
            compaction_history: false,
            context_usage: ContextUsageCapability::Unsupported,
        }
    }
}
