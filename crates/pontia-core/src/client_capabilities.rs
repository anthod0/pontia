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
    pub fn intersection(&self, upper: &Self) -> Self {
        Self {
            accept_task: self.accept_task && upper.accept_task,
            report_turn_started: self.report_turn_started && upper.report_turn_started,
            report_turn_finished: self.report_turn_finished && upper.report_turn_finished,
            interrupt: self.interrupt && upper.interrupt,
            stream_output: self.stream_output && upper.stream_output,
            heartbeat: self.heartbeat && upper.heartbeat,
            timeline: self.timeline && upper.timeline,
            topology: self.topology && upper.topology,
            branch_control: self.branch_control && upper.branch_control,
            list_models: self.list_models && upper.list_models,
            set_model: self.set_model && upper.set_model,
            manual_compaction: self.manual_compaction && upper.manual_compaction,
            compaction_events: self.compaction_events && upper.compaction_events,
            compaction_history: self.compaction_history && upper.compaction_history,
            context_usage: match (self.context_usage, upper.context_usage) {
                (ContextUsageCapability::Unsupported, _)
                | (_, ContextUsageCapability::Unsupported) => ContextUsageCapability::Unsupported,
                (ContextUsageCapability::Estimated, _) | (_, ContextUsageCapability::Estimated) => {
                    ContextUsageCapability::Estimated
                }
                _ => ContextUsageCapability::Exact,
            },
        }
    }

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
