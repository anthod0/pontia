use super::*;

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionCapabilities {
    pub accept_task: bool,
    pub report_turn_started: bool,
    pub report_turn_finished: bool,
    pub interrupt: bool,
    pub stream_output: bool,
    pub heartbeat: bool,
    pub artifact_sources: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SessionView {
    pub session_id: String,
    pub client_type: String,
    pub handle: Option<String>,
    pub role: Option<String>,
    pub description: Option<String>,
    pub execution_profile_id: Option<String>,
    pub execution_profile_version: Option<String>,
    pub state: String,
    pub current_turn_id: Option<String>,
    pub workspace_id: Option<String>,
    pub workspace: Option<String>,
    pub capabilities: SessionCapabilities,
    pub created_at: String,
    pub updated_at: String,
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct WorkspaceView {
    pub workspace_id: String,
    pub canonical_path: String,
    pub display_path: String,
    pub name: Option<String>,
    pub state: String,
    pub metadata: Value,
    pub created_at: String,
    pub updated_at: String,
    pub last_used_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TaskView {
    pub task_id: String,
    pub state: String,
    pub input: String,
    pub workspace_id: Option<String>,
    pub session_id: Option<String>,
    pub turn_id: Option<String>,
    pub routing_state: String,
    pub routing_reason: Option<String>,
    pub routing_confidence: Option<f64>,
    pub metadata: Value,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TaskEventView {
    pub event_id: String,
    pub task_id: String,
    pub event_type: String,
    pub payload: Value,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TurnInputView {
    pub summary: Option<String>,
    pub artifact_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TurnOutputView {
    pub summary: Option<String>,
    pub artifact_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TurnView {
    pub turn_id: String,
    pub session_id: String,
    pub state: String,
    pub input: TurnInputView,
    pub output: TurnOutputView,
    pub failure: Option<String>,
    pub created_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct InboxInputView {
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct InboxMessageView {
    pub message_id: String,
    pub session_id: String,
    pub state: String,
    pub delivery_policy: String,
    pub input: InboxInputView,
    pub metadata: Value,
    pub turn_id: Option<String>,
    pub superseded_by_message_id: Option<String>,
    pub failure_message: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub dispatched_at: Option<String>,
    pub cancelled_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct EventView {
    pub event_id: String,
    pub session_id: String,
    pub turn_id: Option<String>,
    pub source: String,
    #[serde(rename = "type")]
    pub event_type: String,
    pub time: String,
    pub payload: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EventStreamItem {
    pub rowid: i64,
    pub event: EventView,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventStreamScope<'a> {
    Session {
        session_id: &'a str,
    },
    Turn {
        session_id: &'a str,
        turn_id: &'a str,
    },
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ArtifactView {
    pub artifact_id: String,
    pub session_id: String,
    pub turn_id: Option<String>,
    pub kind: String,
    pub name: String,
    pub size_bytes: Option<i64>,
    pub preview: Option<String>,
    pub created_at: String,
    pub metadata: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactContent {
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ArtifactDiscoveryOutcome {
    pub artifacts: Vec<ArtifactView>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct WorkItemEdgeView {
    pub edge_id: String,
    pub task_id: String,
    pub from_work_item_id: String,
    pub to_work_item_id: String,
    pub edge_type: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct WorkItemRuntimeView {
    pub current_run_id: Option<String>,
    pub current_state: String,
    pub current_attempt: i64,
    pub ready_at: Option<String>,
    pub blocked_reason: Option<String>,
    pub retry_count: i64,
    pub max_retries: i64,
    pub priority: i64,
    pub optional: bool,
    pub parallelizable: bool,
    pub session_id: Option<String>,
    pub turn_id: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct WorkItemWithRuntimeView {
    #[serde(flatten)]
    pub work_item: WorkItemRecord,
    pub runtime: Option<WorkItemRuntimeView>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DagSummaryView {
    pub total_work_items: i64,
    pub ready_work_items: i64,
    pub running_work_items: i64,
    pub completed_work_items: i64,
    pub blocked_work_items: i64,
    pub failed_work_items: i64,
    pub open_signals: i64,
    pub total_runs: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TaskDagView {
    pub task_id: String,
    pub summary: DagSummaryView,
    pub work_items: Vec<WorkItemWithRuntimeView>,
    pub edges: Vec<WorkItemEdgeView>,
    pub runs: Vec<WorkItemRunRecord>,
    pub signals: Vec<DagSignalRecord>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct AgentPlanningContextView {
    pub context: AgentToolContext,
    pub mode: &'static str,
    pub role: AgentPlanningRole,
    pub task: TaskView,
    pub dag: TaskDagView,
    pub open_signals: Vec<DagSignalRecord>,
    pub relevant_proposals: Vec<DagProposal>,
    pub execution_profiles: Vec<ExecutionProfileView>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct AgentExecutionContextView {
    pub context: AgentToolContext,
    pub mode: &'static str,
    pub task: TaskView,
    pub work_item: WorkItemWithRuntimeView,
    pub work_item_run: WorkItemRunRecord,
    pub dependencies: Vec<WorkItemEdgeView>,
    pub upstream_completed_items: Vec<WorkItemWithRuntimeView>,
    pub acceptance_criteria: Value,
    pub open_signals: Vec<DagSignalRecord>,
}
