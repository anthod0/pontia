#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SessionRow {
    pub session_id: String,
    pub client_type: String,
    pub title: Option<String>,
    pub handle: Option<String>,
    pub role: Option<String>,
    pub description: Option<String>,
    pub execution_profile_id: Option<String>,
    pub execution_profile_version: Option<String>,
    pub state: String,
    pub current_turn_id: Option<String>,
    pub workspace_id: Option<String>,
    pub workspace_ref: Option<String>,
    pub metadata: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RuntimeBindingMetadataRow {
    pub metadata: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SessionProjectionRow {
    pub session_id: String,
    pub client_type: String,
    pub title: Option<String>,
    pub handle: Option<String>,
    pub role: Option<String>,
    pub description: Option<String>,
    pub execution_profile_id: Option<String>,
    pub execution_profile_version: Option<String>,
    pub state: String,
    pub current_turn_id: Option<String>,
    pub state_version: i64,
    pub metadata: String,
}
