#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ArtifactRow {
    pub artifact_id: String,
    pub session_id: String,
    pub turn_id: Option<String>,
    pub kind: String,
    pub name: String,
    pub size_bytes: Option<i64>,
    pub metadata: String,
    pub created_at: String,
}
