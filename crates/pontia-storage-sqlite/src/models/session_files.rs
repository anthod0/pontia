#[derive(Debug, Clone, PartialEq, serde::Serialize, sqlx::FromRow)]
pub struct SharedFile {
    pub file_id: String,
    pub turn_id: Option<String>,
    pub path: String,
    pub name: String,
    pub size: i64,
    pub media_type: String,
    pub created_at: String,
}
