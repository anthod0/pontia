#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct WorkflowRow {
    pub workflow_id: String,
    pub title: String,
    pub cwd: String,
    pub state: String,
    pub current_revision: i64,
    pub failure_message: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct WorkflowNodeRow {
    pub node_id: String,
    pub workflow_id: String,
    pub parent_node_id: Option<String>,
    pub node_type: String,
    pub phase: String,
    pub title: String,
    pub instructions: String,
    pub inputs: String,
    pub output: String,
    pub execution_profile_id: Option<String>,
    pub execution_profile_version: Option<String>,
    pub introduced_revision: i64,
    pub retired_revision: Option<i64>,
    pub session_id: Option<String>,
    pub submitted_at: Option<String>,
    pub submitted_runtime_instance_id: Option<String>,
    pub exit_request_started_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct WorkflowEventRow {
    pub event_id: String,
    pub workflow_id: String,
    pub sequence: i64,
    pub event_type: String,
    pub payload: String,
    pub created_at: String,
}
