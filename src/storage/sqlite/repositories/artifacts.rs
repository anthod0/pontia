use sqlx::SqlitePool;

use pontia_storage_sqlite::models::artifacts::ArtifactRow;

use crate::error::Result;

#[derive(Debug, Clone)]
pub struct SqliteArtifactRepository {
    pool: SqlitePool,
}

impl SqliteArtifactRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn list_artifacts(&self, session_id: &str) -> Result<Vec<ArtifactRow>> {
        Ok(sqlx::query_as::<_, ArtifactRow>(
            r#"SELECT artifact_id, session_id, turn_id, kind, name, size_bytes, metadata, created_at
               FROM artifacts WHERE session_id = ? ORDER BY created_at, artifact_id"#,
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn get_artifact(&self, artifact_id: &str) -> Result<Option<ArtifactRow>> {
        Ok(sqlx::query_as::<_, ArtifactRow>(
            r#"SELECT artifact_id, session_id, turn_id, kind, name, size_bytes, metadata, created_at
               FROM artifacts WHERE artifact_id = ?"#,
        )
        .bind(artifact_id)
        .fetch_optional(&self.pool)
        .await?)
    }
}
