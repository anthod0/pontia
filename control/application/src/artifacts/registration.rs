use sqlx::SqlitePool;

use pontia_core::error::Result;
use pontia_storage_sqlite::repositories::artifacts::{
    ArtifactUpsertRecord, SqliteArtifactRepository,
};

use super::ArtifactRegistration;

#[derive(Clone)]
pub struct ArtifactRegistrationService {
    pool: SqlitePool,
}

impl ArtifactRegistrationService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn register(&self, artifact: ArtifactRegistration) -> Result<()> {
        SqliteArtifactRepository::new(self.pool.clone())
            .upsert_artifact(ArtifactUpsertRecord {
                artifact_id: artifact.artifact_id,
                session_id: artifact.session_id,
                turn_id: artifact.turn_id,
                kind: artifact.kind,
                name: artifact.name,
                source_ref: artifact.source_ref,
                size_bytes: artifact.size_bytes,
                metadata: serde_json::to_string(&artifact.metadata)?,
            })
            .await
    }
}
