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
    pub seq: Option<i64>,
    pub payload: String,
}
