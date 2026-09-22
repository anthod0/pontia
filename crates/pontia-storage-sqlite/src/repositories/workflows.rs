use sqlx::SqlitePool;

mod definition;
mod events;
mod lifecycle;
mod nodes;
mod patch_continuation;
mod patch_planning;
mod patch_queries;
mod patch_resolution;
mod queries;
mod reporting_failure;

#[derive(Debug, Clone)]
pub struct CreateWorkflowRecord {
    pub workflow_id: String,
    pub title: String,
    pub cwd: String,
    pub state: String,
}

#[derive(Debug, Clone)]
pub struct CreateWorkflowNodeRecord {
    pub node_id: String,
    pub workflow_id: String,
    pub parent_node_id: Option<String>,
    pub phase: String,
    pub title: String,
    pub instructions: String,
    pub inputs: String,
    pub output: String,
    pub execution_profile_id: Option<String>,
    pub execution_profile_version: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SqliteWorkflowRepository {
    pool: SqlitePool,
}

#[derive(Debug, Clone)]
pub struct RequestWorkflowPatchRecord {
    pub patch_id: String,
    pub session_id: String,
    pub runtime_instance_id: String,
    pub request_document_ref: String,
    pub request_size_bytes: i64,
    pub replanner_creation_token: String,
    pub event_id: String,
}

#[derive(Debug, Clone)]
pub struct ApplyWorkflowNodeRecord {
    pub node_id: String,
    pub parent_node_id: Option<String>,
    pub phase: String,
    pub title: String,
    pub instructions: String,
    pub inputs: String,
    pub output: String,
    pub execution_profile_id: Option<String>,
    pub execution_profile_version: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ApplyWorkflowPatchRecord {
    pub session_id: String,
    pub runtime_instance_id: String,
    pub decision_document_ref: String,
    pub decision_size_bytes: i64,
    pub decision_summary: String,
    pub retired_node_ids: Vec<String>,
    pub introduced_nodes: Vec<ApplyWorkflowNodeRecord>,
    pub continuation_message_id: String,
    pub event_id: String,
}

#[derive(Debug, Clone)]
pub struct BlockWorkflowPatchRecord {
    pub session_id: String,
    pub runtime_instance_id: String,
    pub reason_document_ref: String,
    pub blocked_draft_ref: Option<String>,
    pub event_id: String,
}

#[derive(Debug, Clone)]
pub struct ImplicitBlockWorkflowPatchRecord {
    pub patch_id: String,
    pub replanner_turn_id: Option<String>,
    pub reason_document_ref: String,
    pub blocked_draft_ref: Option<String>,
    pub reason_summary: String,
    pub event_id: String,
}

impl SqliteWorkflowRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}
