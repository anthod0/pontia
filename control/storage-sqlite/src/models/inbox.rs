#[derive(Debug, Clone, sqlx::FromRow)]
pub struct InboxMessageRow {
    pub message_id: String,
    pub session_id: String,
    pub state: String,
    pub delivery_policy: String,
    pub input_summary: String,
    pub metadata: String,
    pub turn_id: Option<String>,
    pub superseded_by_message_id: Option<String>,
    pub failure_message: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub dispatched_at: Option<String>,
    pub cancelled_at: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PendingInboxMessageRow {
    pub message_id: String,
    pub input_summary: String,
    pub metadata: String,
}
