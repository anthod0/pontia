#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AgentProfileRow {
    pub profile_id: String,
    pub name: String,
    pub description: Option<String>,
    pub client_type: Option<String>,
    pub client_key: Option<String>,
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
    pub system_prompt: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}
