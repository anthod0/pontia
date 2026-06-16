#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TurnRow {
    pub turn_id: String,
    pub session_id: String,
    pub state: String,
    pub input_summary: Option<String>,
    pub output_summary: Option<String>,
    pub failure_message: Option<String>,
    pub metadata: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TurnEventEnrichmentRow {
    pub event_id: String,
    pub event_type: String,
    pub occurred_at: String,
    pub payload: String,
}
