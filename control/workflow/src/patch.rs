use std::path::{Component, Path, PathBuf};

use pontia_storage_sqlite::repositories::workflows::{
    BlockWorkflowPatchRecord, RequestWorkflowPatchRecord, SqliteWorkflowRepository,
};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::{
    BlockWorkflowPatch, BlockWorkflowPatchOutcome, Error, RequestWorkflowPatch,
    RequestWorkflowPatchOutcome, Result,
};

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

    pub async fn block_patch(
        &self,
        request: BlockWorkflowPatch,
    ) -> Result<BlockWorkflowPatchOutcome> {
        if request.reason.trim().is_empty() {
            return Err(Error::InvalidDefinition(
                "Workflow Patch block reason must not be empty".into(),
            ));
        }
        validate_pontia_home_boundary(&self.pontia_home)?;
        let patch = self
            .repository
            .get_active_patch_for_replanner(&request.session_id, &request.runtime_instance_id)
            .await?
            .ok_or_else(|| {
                pontia_core::Error::StateConflict(format!(
                    "session {} is not the active Workflow Re-planner",
                    request.session_id
                ))
            })?;
        let workflow_dir = self.pontia_home.join("workflows").join(&patch.workflow_id);
        let patch_dir = workflow_dir.join("patches").join(&patch.patch_id);
        self.validate_patch_directory(&workflow_dir, &patch_dir)?;

        self.write_atomic(&patch_dir, "reason.md", request.reason.as_bytes())
            .await?;
        let workflow_file = workflow_dir.join("workflow.toml");
        let accepted_file = patch_dir.join("accepted-definition.toml");
        for path in [&accepted_file, &workflow_file] {
            let metadata = std::fs::symlink_metadata(path)?;
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(Error::InvalidWorkflowId(path.display().to_string()));
            }
        }
        let accepted = tokio::fs::read(&accepted_file).await?;
        let draft = tokio::fs::read(&workflow_file).await?;
        let blocked_draft_ref = if draft != accepted {
            self.write_atomic(&patch_dir, "blocked-draft.toml", &draft)
                .await?;
            Some(format!("patches/{}/blocked-draft.toml", patch.patch_id))
        } else {
            None
        };
        let reason_document_ref = format!("patches/{}/reason.md", patch.patch_id);
        let blocked = self
            .repository
            .block_patch(BlockWorkflowPatchRecord {
                session_id: request.session_id,
                runtime_instance_id: request.runtime_instance_id,
                reason_document_ref,
                blocked_draft_ref,
                event_id: Uuid::now_v7().to_string(),
            })
            .await?;
        self.write_atomic(&workflow_dir, "workflow.toml", &accepted)
            .await?;
        Ok(BlockWorkflowPatchOutcome {
            patch_id: blocked.patch_id,
            workflow_id: blocked.workflow_id,
        })
    }

    async fn write_atomic(&self, directory: &Path, name: &str, content: &[u8]) -> Result<()> {
        let pending = directory.join(format!(".{name}.tmp"));
        tokio::fs::write(&pending, content).await?;
        if let Err(error) = tokio::fs::rename(&pending, directory.join(name)).await {
            let _ = tokio::fs::remove_file(&pending).await;
            return Err(error.into());
        }
        Ok(())
    }

    fn validate_patch_directory(&self, workflow_dir: &Path, patch_dir: &Path) -> Result<()> {
        for path in [
            self.pontia_home.join("workflows"),
            workflow_dir.to_path_buf(),
            patch_dir.to_path_buf(),
        ] {
            let metadata = std::fs::symlink_metadata(&path)?;
            if metadata.file_type().is_symlink() {
                return Err(Error::InvalidWorkflowId(path.display().to_string()));
            }
        }
        Ok(())
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
