#[derive(Debug, Clone, sqlx::FromRow)]
pub struct WorkspaceRow {
    pub workspace_id: String,
    pub canonical_path: String,
    pub display_path: String,
    pub name: Option<String>,
    pub state: String,
    pub metadata: String,
    pub created_at: String,
    pub updated_at: String,
    pub last_used_at: Option<String>,
}
