use pontia_core::error::{Error, Result};
use pontia_runtime::RuntimeStartResult;
use pontia_storage_sqlite::repositories::{
    runtime_bindings::SqliteRuntimeBindingRepository, sessions::SqliteSessionRepository,
};

use super::SessionCommandService;
use crate::WorkspaceRecord;

impl SessionCommandService {
    pub(super) async fn ensure_handle_available(
        &self,
        workspace_id: &str,
        handle: &str,
    ) -> Result<()> {
        if SqliteSessionRepository::new(self.pool.clone())
            .active_session_id_for_handle(workspace_id, handle)
            .await?
            .is_some()
        {
            return Err(Error::Conflict {
                code: "session_handle_conflict",
                message: format!(
                    "Cannot create session because {handle} is already used, please try a different handle."
                ),
            });
        }

        Ok(())
    }

    pub(super) async fn upsert_runtime_binding(
        &self,
        session_id: &str,
        runtime: &RuntimeStartResult,
    ) -> Result<()> {
        let result = SqliteRuntimeBindingRepository::new(self.pool.clone())
            .upsert_binding_guarded(crate::runtime_control::runtime_binding_record(
                session_id, runtime,
            )?)
            .await;
        if result.is_err() {
            let _ = self.runtime.terminate_session(&runtime.runtime_handle);
        }
        result
    }

    pub(super) async fn update_session_workspace(
        &self,
        session_id: &str,
        workspace: Option<&WorkspaceRecord>,
    ) -> Result<()> {
        SqliteSessionRepository::new(self.pool.clone())
            .update_session_workspace(
                session_id,
                workspace.map(|workspace| workspace.canonical_path.as_str()),
                workspace.map(|workspace| workspace.workspace_id.as_str()),
            )
            .await
    }
}
