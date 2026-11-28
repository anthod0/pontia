use crate::runtime::ControlTarget;
use sqlx::SqlitePool;

#[derive(Clone)]
pub struct ClientControlStore {
    pool: SqlitePool,
}

impl ClientControlStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
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
