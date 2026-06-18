#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct AgentBindingRow {
    pub id: String,
    pub session_id: String,
    pub client_type: String,
    pub launch_cwd: String,
    pub client_session_key: String,
    pub metadata: String,
    pub discovered: bool,
}
