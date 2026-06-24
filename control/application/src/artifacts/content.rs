use sqlx::SqlitePool;

use pontia_core::error::{Error, Result};
use pontia_storage_sqlite::repositories::artifacts::SqliteArtifactRepository;

use super::helpers::{MAX_ARTIFACT_CONTENT_BYTES, artifact_file_path};
use crate::ArtifactContent;

#[derive(Clone)]
pub struct ArtifactContentService {
    pool: SqlitePool,
}

impl ArtifactContentService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn read_content(&self, artifact_id: &str) -> Result<ArtifactContent> {
        let row = SqliteArtifactRepository::new(self.pool.clone())
            .artifact_source(artifact_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("artifact {artifact_id} not found")))?;

        let source_ref = row.source_ref;
        let expected_size = row.size_bytes;
        if let Some(expected_size) = expected_size
            && expected_size > MAX_ARTIFACT_CONTENT_BYTES
        {
            return Err(Error::Domain(format!(
                "artifact {artifact_id} is too large to read through the content API"
            )));
        }
        let path = artifact_file_path(&source_ref)?;
        let bytes = std::fs::read(&path).map_err(|err| {
            if err.kind() == std::io::ErrorKind::NotFound {
                Error::NotFound(format!(
                    "artifact {artifact_id} content not found at registered source"
                ))
            } else {
                Error::Io(err)
            }
        })?;

        if let Some(expected_size) = expected_size
            && expected_size >= 0
            && bytes.len() as i64 != expected_size
        {
            return Err(Error::StateConflict(format!(
                "artifact {artifact_id} metadata size {expected_size} does not match content size {}",
                bytes.len()
            )));
        }

        Ok(ArtifactContent { bytes })
    }
}
