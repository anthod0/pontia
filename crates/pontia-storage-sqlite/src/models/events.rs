#[derive(Debug, Clone, sqlx::FromRow)]
pub struct EventRow {
    pub event_id: String,
    pub session_id: String,
    pub turn_id: Option<String>,
    pub source: String,
    pub event_type: String,
    pub occurred_at: String,
    pub payload: String,
}

/// Persisted lifecycle evidence with its historical Runtime correlation.
/// Turn Runtime identity comes from the confirmed start, not the terminal payload.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct WorkflowTerminalEventRow {
    pub event_id: String,
    pub turn_id: Option<String>,
    pub event_type: String,
    pub runtime_instance_id: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct EventStreamRow {
    pub rowid: i64,
    pub event_id: String,
    pub session_id: String,
    pub turn_id: Option<String>,
    pub source: String,
    pub event_type: String,
    pub occurred_at: String,
    pub payload: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TaskEventStreamRow {
    pub rowid: i64,
    pub event_id: String,
    pub task_id: String,
    pub event_type: String,
    pub payload: String,
    pub created_at: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DomainEventRow {
    pub event_id: String,
    pub session_id: String,
    pub turn_id: Option<String>,
    pub source: String,
    pub client_type: String,
    pub event_type: String,
    pub occurred_at: String,
    pub payload: String,
    pub timeline_boundary: Option<String>,
    pub turn_topology: Option<String>,
}
