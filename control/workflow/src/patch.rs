use std::path::{Component, Path, PathBuf};

use pontia_storage_sqlite::repositories::workflows::{
    RequestWorkflowPatchRecord, SqliteWorkflowRepository,
};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::{Error, RequestWorkflowPatch, RequestWorkflowPatchOutcome, Result};

pub struct WorkflowPatchService {
    repository: SqliteWorkflowRepository,
    pontia_home: PathBuf,
}

impl WorkflowPatchService {
    pub fn new(pool: SqlitePool, pontia_home: PathBuf) -> Self {
        Self {
            repository: SqliteWorkflowRepository::new(pool),
            pontia_home,
        }
    }

    pub async fn request_patch(
        &self,
        request: RequestWorkflowPatch,
    ) -> Result<RequestWorkflowPatchOutcome> {
        validate_pontia_home_boundary(&self.pontia_home)?;
        let node = self
            .repository
            .get_node_by_session(&request.session_id)
            .await?
            .ok_or_else(|| Error::NodeForSessionNotFound(request.session_id.clone()))?;
        let patch_id = format!("patch_{}", Uuid::now_v7());
        let request_document_ref = format!("patches/{patch_id}/request.md");
        let patch_dir = self
            .pontia_home
            .join("workflows")
            .join(&node.workflow_id)
            .join("patches")
            .join(&patch_id);
        self.write_request_document(&patch_dir, &request.document)
            .await?;

        let accepted = self
            .repository
            .request_patch(RequestWorkflowPatchRecord {
                patch_id: patch_id.clone(),
                session_id: request.session_id,
                runtime_instance_id: request.runtime_instance_id,
                request_document_ref,
                request_size_bytes: i64::try_from(request.document.len()).map_err(|_| {
                    Error::InvalidDefinition("Workflow Patch request document is too large".into())
                })?,
                replanner_creation_token: format!("workflow_replanner_{}", Uuid::now_v7()),
                event_id: Uuid::now_v7().to_string(),
            })
            .await;
        match accepted {
            Ok(patch) => Ok(RequestWorkflowPatchOutcome {
                patch_id: patch.patch_id,
            }),
            Err(error) => {
                self.remove_unaccepted_document(&patch_dir).await;
                Err(error.into())
            }
        }
    }

    async fn write_request_document(&self, patch_dir: &Path, document: &str) -> Result<()> {
        let workflow_dir = patch_dir
            .parent()
            .and_then(Path::parent)
            .ok_or_else(|| Error::InvalidWorkflowId(patch_dir.display().to_string()))?;
        let workflows_dir = self.pontia_home.join("workflows");
        let workflow_file = workflow_dir.join("workflow.toml");
        for path in [
            workflows_dir.as_path(),
            workflow_dir,
            workflow_file.as_path(),
        ] {
            let metadata = std::fs::symlink_metadata(path)?;
            if metadata.file_type().is_symlink() {
                return Err(Error::InvalidWorkflowId(path.display().to_string()));
            }
        }
        let patches_dir = workflow_dir.join("patches");
        match std::fs::symlink_metadata(&patches_dir) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(Error::InvalidWorkflowId(patches_dir.display().to_string()));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                tokio::fs::create_dir(&patches_dir).await?;
            }
            Err(error) => return Err(error.into()),
        }
        tokio::fs::create_dir(patch_dir).await?;
        let pending = patch_dir.join(".request.md.tmp");
        if let Err(error) = tokio::fs::write(&pending, document).await {
            self.remove_unaccepted_document(patch_dir).await;
            return Err(error.into());
        }
        if let Err(error) = tokio::fs::rename(&pending, patch_dir.join("request.md")).await {
            self.remove_unaccepted_document(patch_dir).await;
            return Err(error.into());
        }
        Ok(())
    }

    async fn remove_unaccepted_document(&self, patch_dir: &Path) {
        let _ = tokio::fs::remove_file(patch_dir.join(".request.md.tmp")).await;
        let _ = tokio::fs::remove_file(patch_dir.join("request.md")).await;
        let _ = tokio::fs::remove_dir(patch_dir).await;
    }
}

fn validate_pontia_home_boundary(pontia_home: &Path) -> Result<()> {
    if pontia_home.as_os_str().is_empty()
        || !pontia_home.is_absolute()
        || pontia_home.parent().is_none()
        || pontia_home
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(Error::InvalidWorkflowId(pontia_home.display().to_string()));
    }
    match std::fs::symlink_metadata(pontia_home) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(Error::InvalidWorkflowId(pontia_home.display().to_string()))
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}
