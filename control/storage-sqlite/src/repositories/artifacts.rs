use sqlx::SqlitePool;

use crate::models::artifacts::ArtifactRow;

use pontia_core::Result;

#[derive(Debug, Clone)]
pub struct ArtifactUpsertRecord {
    pub artifact_id: String,
    pub session_id: String,
    pub turn_id: Option<String>,
    pub kind: String,
    pub name: String,
    pub source_ref: String,
    pub size_bytes: Option<i64>,
    pub metadata: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ArtifactSourceRow {
    pub source_ref: String,
    pub size_bytes: Option<i64>,
}

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

    pub async fn upsert_artifact(&self, artifact: ArtifactUpsertRecord) -> Result<()> {
        sqlx::query(
            r#"INSERT INTO artifacts
               (artifact_id, session_id, turn_id, kind, name, source_ref, size_bytes, metadata)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?)
               ON CONFLICT(artifact_id) DO UPDATE SET
                   session_id = excluded.session_id,
                   turn_id = excluded.turn_id,
                   kind = excluded.kind,
                   name = excluded.name,
                   source_ref = excluded.source_ref,
                   size_bytes = excluded.size_bytes,
                   metadata = excluded.metadata,
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')"#,
        )
        .bind(artifact.artifact_id)
        .bind(artifact.session_id)
        .bind(artifact.turn_id)
        .bind(artifact.kind)
        .bind(artifact.name)
        .bind(artifact.source_ref)
        .bind(artifact.size_bytes)
        .bind(artifact.metadata)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn artifact_source(&self, artifact_id: &str) -> Result<Option<ArtifactSourceRow>> {
        Ok(sqlx::query_as::<_, ArtifactSourceRow>(
            "SELECT source_ref, size_bytes FROM artifacts WHERE artifact_id = ?",
        )
        .bind(artifact_id)
        .fetch_optional(&self.pool)
        .await?)
    }
}
