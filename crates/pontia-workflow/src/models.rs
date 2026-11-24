#[derive(Debug, Clone, PartialEq, sqlx::FromRow, serde::Serialize)]
pub struct WorkflowRecoveryRow {
    pub recovery_id: String,
    pub workflow_id: String,
    pub failure_event_id: String,
    pub exit_event_id: String,
    pub node_id: String,
    pub session_id: String,
    pub message_id: String,
    pub state: String,
    pub runtime_id: Option<String>,
    pub failure_message: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct WorkflowRow {
    pub workflow_id: String,
    pub title: String,
    pub cwd: String,
    pub state: String,
    pub initiator_session_id: String,
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
    pub session_id: Option<String>,
    pub submitted_at: Option<String>,
    pub submitted_runtime_id: Option<String>,
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

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct WorkflowAgentEventRow {
    pub rowid: i64,
    pub event_id: String,
    pub session_id: String,
    pub turn_id: Option<String>,
    pub source: String,
    pub event_type: String,
    pub occurred_at: String,
    pub payload: String,
    pub created_at: String,
}

/// Persisted lifecycle evidence with its historical Runtime correlation.
/// Turn Runtime identity comes from the confirmed start, not the terminal payload.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct WorkflowTerminalEventRow {
    pub event_id: String,
    pub turn_id: Option<String>,
    pub event_type: String,
    pub runtime_id: Option<String>,
}
