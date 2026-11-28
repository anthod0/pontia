use crate::runtime::ControlTarget;
use sqlx::SqlitePool;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Clone)]
pub struct ClientControlStore {
    pub(crate) pool: SqlitePool,
    managed_private_roots: Arc<[PathBuf]>,
}

impl ClientControlStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self::with_managed_private_roots(pool, Vec::new())
    }

    pub(crate) fn with_managed_private_roots(pool: SqlitePool, roots: Vec<PathBuf>) -> Self {
        Self {
            pool,
            managed_private_roots: roots.into(),
        }
    }

    pub fn require_managed_launch_directory(&self, directory: &Path) -> pontia_core::Result<()> {
        crate::workspaces::require_managed_launch_directory(&self.managed_private_roots, directory)
    }

    pub(crate) async fn resolve_session_directory(
        &self,
        directory: &str,
    ) -> pontia_core::Result<(String, Option<crate::WorkspaceRecord>)> {
        crate::workspaces::resolve_session_directory(
            &self.pool,
            &self.managed_private_roots,
            directory,
        )
        .await
    }

    pub fn queries(&self) -> crate::ControlQueryService {
        crate::ControlQueryService::new(self.pool.clone())
    }

    pub fn bindings(&self) -> crate::AgentBindingService {
        crate::AgentBindingService::new(self.pool.clone())
    }

    pub fn runtime_attachments(&self) -> crate::runtime::RuntimeAttachmentService {
        crate::runtime::RuntimeAttachmentService::new(self.pool.clone())
    }

    pub fn instructions(&self) -> crate::SessionInstructionsService {
        crate::SessionInstructionsService::new(self.pool.clone())
    }

    pub fn metadata(&self) -> crate::SessionMetadataStore {
        crate::SessionMetadataStore::new(self.pool.clone())
    }

    pub fn runtime_readiness(&self, role: Option<&'static str>) -> crate::RuntimeReadinessService {
        crate::RuntimeReadinessService::new(self.pool.clone(), role)
    }

    pub async fn validate_target(&self, target: &ControlTarget) -> pontia_core::Result<()> {
        target.validate(&self.pool).await
    }

    pub async fn require_active_workspace(
        &self,
        cwd: &str,
    ) -> pontia_core::Result<crate::WorkspaceRecord> {
        crate::workspaces::require_active_workspace(&self.pool, cwd).await
    }
}
