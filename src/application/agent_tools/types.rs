use super::*;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentToolRequest {
    pub session_id: String,
    pub turn_id: String,
    pub runtime_instance_id: String,
    #[serde(default = "super::default_agent_tool_input")]
    pub input: Value,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(untagged)]
pub enum AgentToolResponse {
    Skeleton { context: AgentToolContext },
    GetContext(GetContextToolResponse),
    SubmitPlan(SubmitPlanToolResponse),
    ApplyPlan(ApplyPlanToolResponse),
    SubmitResult(SubmitResultToolResponse),
    RaiseSignal(RaiseSignalToolResponse),
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct GetContextToolResponse {
    pub text: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SubmitPlanToolResponse {
    pub proposal_id: String,
    pub validation: Value,
    pub apply: Value,
    pub scheduler: DagSchedulerOutcome,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ApplyPlanToolResponse {
    pub proposal_id: String,
    pub validation: Value,
    pub apply: Value,
    pub scheduler: DagSchedulerOutcome,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SubmitResultToolResponse {
    pub task_id: String,
    pub work_item_id: String,
    pub run_id: String,
    pub state: String,
    pub scheduler: DagSchedulerOutcome,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RaiseSignalToolResponse {
    pub signal_id: String,
    pub task_id: String,
    pub work_item_id: Option<String>,
    pub run_id: Option<String>,
    pub kind: String,
    pub state: String,
    pub policy: Value,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct AgentToolContext {
    pub session_id: String,
    pub turn_id: String,
    pub client_type: String,
    pub runtime_instance_id: String,
    pub task_id: String,
    pub mode: AgentToolMode,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentToolMode {
    Planning {
        role: AgentPlanningRole,
    },
    Execution {
        run_id: String,
        work_item_id: String,
    },
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentPlanningRole {
    Planner,
    Replanner,
}
