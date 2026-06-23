use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{Row, SqlitePool};

use pontia_agent_clients as agent_clients;
use pontia_application::{
    CreateSessionRequest, ExternalQueryService, RuntimeControlService, SessionCommandService,
    TaskView, TurnCommandService,
};
use pontia_core::{
    domain::{DomainEvent, EventType},
    error::{Error, Result},
    ids::new_event_id,
};

mod agent_tools;
mod dag;
mod dag_models;
mod dag_planning;
mod dag_run_result;
mod dag_scheduler;
mod dag_validator;
mod graph;
mod mapping;
pub mod profiles;
mod prompt_rendering;
mod queries_dag;
mod tasks;

pub use agent_tools::{
    AgentPlanningRole, AgentToolContext, AgentToolContextResolver, AgentToolMode, AgentToolRequest,
    AgentToolResponse, AgentToolService,
};
pub use dag::DagService;
pub use dag_models::{
    DagPatch, DagPatchApplySummary, DagProposal, DagSignalRecord, PatchOperation,
    RaiseSignalPayload, SubmitPlanPayload, SubmitResultPayload, WorkItemDraft, WorkItemEdgeDraft,
    WorkItemRecord, WorkItemRunRecord,
};
pub use dag_planning::{DagPlanningOutcome, DagPlanningService, DagPlanningTurn};
pub use dag_run_result::DagRunResultService;
pub use dag_scheduler::{DagSchedulerDispatch, DagSchedulerOutcome, DagSchedulerService};
#[cfg(feature = "lbug")]
pub use graph::LbugDagGraphStore;
pub use graph::{
    AddWorkItemEdgeRequest, GraphEdgeKind, GraphProjectionService, SignalNode, TaskGraphSnapshot,
    TaskNode, TaskProvenance, UpsertSignalRequest, UpsertTaskRequest, UpsertWorkItemRequest,
    WorkItemEdgeRecord, WorkItemNode,
};
pub use pontia_config::GraphRuntimeConfig;
pub use profiles::{
    AgentProfileCommandOutcome, AgentProfileService, ExecutionProfileView,
    UpsertExecutionProfileRequest,
};
pub use queries_dag::DagQueryService;
pub use tasks::{
    CreateDagTaskRequest, DagTaskCommandOutcome, DagTaskCommandService, HumanSignalRequest,
};

pub(crate) use mapping::*;

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

fn default_client_type() -> String {
    agent_clients::default_real_client_type().to_string()
}

fn is_supported_client_type(client_type: &str) -> bool {
    agent_clients::is_supported_client_type(client_type)
}

pub(crate) fn nested_string(payload: &Value, path: &[&str]) -> Option<String> {
    let mut current = payload;
    for segment in path {
        current = current.get(*segment)?;
    }
    current.as_str().map(ToString::to_string)
}
