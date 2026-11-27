#[derive(Debug, Clone, sqlx::FromRow)]
pub struct InboxMessageRow {
    pub message_id: String,
    pub session_id: String,
    pub input_summary: String,
    pub metadata: String,
    pub created_at: String,
}
