use serde_json::json;
use sqlx::SqlitePool;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use pontia_core::error::{Error, Result};
use pontia_storage_sqlite::repositories::artifacts::{
    ArtifactUpsertRecord, SqliteArtifactRepository,
};

use super::helpers::{
    collect_workspace_files, content_fingerprint, deterministic_artifact_id, infer_artifact_kind,
    preview_for_file,
};
use crate::{ArtifactDiscoveryOutcome, ExternalQueryService};

#[derive(Clone)]
pub struct ArtifactDiscoveryService {
    pool: SqlitePool,
}

impl ArtifactDiscoveryService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn discover(&self, session_id: &str) -> Result<ArtifactDiscoveryOutcome> {
        let query = ExternalQueryService::new(self.pool.clone());
        let session = query
            .get_session(session_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("session {session_id} not found")))?;
        let workspace = session.workspace.ok_or_else(|| {
            Error::Domain(format!(
                "session {session_id} does not have a workspace to discover"
            ))
        })?;
        let workspace_root = std::fs::canonicalize(&workspace).map_err(|err| {
            if err.kind() == std::io::ErrorKind::NotFound {
                Error::NotFound(format!("session {session_id} workspace not found"))
            } else {
                Error::Io(err)
            }
        })?;
        if !workspace_root.is_dir() {
            return Err(Error::Domain(format!(
                "session {session_id} workspace is not a directory"
            )));
        }

        let mut discovered = Vec::new();
        collect_workspace_files(&workspace_root, &workspace_root, &mut discovered)?;
        discovered.sort();

        for path in discovered {
            let relative_path = path
                .strip_prefix(&workspace_root)
                .map_err(|_| Error::Domain("artifact path escaped workspace".to_string()))?
                .to_string_lossy()
                .replace('\\', "/");
            let metadata = std::fs::metadata(&path)?;
            let size_bytes = i64::try_from(metadata.len()).map_err(|_| {
                Error::Domain(format!("artifact {relative_path} is too large to index"))
            })?;
            let preview = preview_for_file(&path)?;
            let kind = infer_artifact_kind(&path, preview.as_deref());
            let modified_at = metadata
                .modified()
                .ok()
                .and_then(|time| OffsetDateTime::from(time).format(&Rfc3339).ok());
            let artifact_id = deterministic_artifact_id(session_id, &relative_path);
            let source_ref = format!("file://{}", path.display());
            let metadata = json!({
                "relative_path": relative_path,
                "modified_at": modified_at,
                "content_fingerprint": content_fingerprint(session_id, size_bytes, &source_ref),
                "preview": preview,
                "source_ref": source_ref,
            });

            SqliteArtifactRepository::new(self.pool.clone())
                .upsert_artifact(ArtifactUpsertRecord {
                    artifact_id,
                    session_id: session_id.to_string(),
                    turn_id: None,
                    kind: kind.to_string(),
                    name: relative_path,
                    source_ref,
                    size_bytes: Some(size_bytes),
                    metadata: serde_json::to_string(&metadata)?,
                })
                .await?;
        }

        let artifacts = query.list_artifacts(session_id).await?;
        Ok(ArtifactDiscoveryOutcome { artifacts })
    }
}
