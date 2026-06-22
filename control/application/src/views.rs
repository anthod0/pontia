use super::*;

pub use pontia_agent_clients::ContextUsageCapability;
pub type SessionCapabilities = pontia_agent_clients::AgentClientCapabilities;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContextUsageView {
    pub used_tokens: Option<u64>,
    pub max_tokens: Option<u64>,
    pub remaining_tokens: Option<u64>,
    pub usage_ratio: Option<f64>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cache_tokens: Option<u64>,
    pub confidence: String,
    pub observed_at: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SessionLineageView {
    pub relation_type: String,
    pub parent_session_id: String,
    pub forked_from_turn_id: Option<String>,
    pub forked_from_client_node_id: Option<String>,
    pub parent_client_session_key: Option<String>,
    pub child_client_session_key: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SessionView {
    pub session_id: String,
    pub client_type: String,
    pub title: Option<String>,
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
    pub model: Option<String>,
    pub context_usage: Option<ContextUsageView>,
    pub lineage: Option<SessionLineageView>,
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
pub struct WorkspaceGitStatusView {
    pub workspace_id: String,
    pub repo_root: Option<String>,
    pub branch: Option<String>,
    pub upstream: Option<String>,
    pub ahead: i64,
    pub behind: i64,
    pub staged_count: i64,
    pub unstaged_count: i64,
    pub untracked_count: i64,
    pub conflicted_count: i64,
    pub clean: bool,
    pub state: String,
    pub failure: Option<String>,
    pub observed_at: Option<String>,
    pub updated_at: Option<String>,
}

impl WorkspaceGitStatusView {
    pub fn unknown(workspace_id: &str) -> Self {
        Self {
            workspace_id: workspace_id.to_string(),
            repo_root: None,
            branch: None,
            upstream: None,
            ahead: 0,
            behind: 0,
            staged_count: 0,
            unstaged_count: 0,
            untracked_count: 0,
            conflicted_count: 0,
            clean: true,
            state: "unknown".to_string(),
            failure: None,
            observed_at: None,
            updated_at: None,
        }
    }
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
pub struct DagProposalView {
    pub proposal_id: String,
    pub task_id: String,
    pub mode: String,
    pub state: String,
    pub summary: String,
    pub proposal_json: Value,
    pub validation_json: Value,
    pub created_by_session_id: Option<String>,
    pub created_by_turn_id: String,
    pub revision: i64,
    pub supersedes_proposal_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
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

#[derive(Debug, Clone, PartialEq)]
pub struct TaskEventStreamItem {
    pub rowid: i64,
    pub event: TaskEventView,
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
    pub outcome_state: Option<String>,
    pub outcome_reason: Option<String>,
    pub replanned_from_state: Option<String>,
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
