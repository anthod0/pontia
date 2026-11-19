use pontia_core::error::{Error, Result};
use pontia_runtime::RuntimeStartResult;
use pontia_storage_sqlite::repositories::{
    session_runtimes::SqliteSessionRuntimeRepository, sessions::SqliteSessionRepository,
};

use super::SessionCommandService;

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
        role: Option<&str>,
    ) -> Result<()> {
        let result = SqliteSessionRuntimeRepository::new(self.pool.clone())
            .upsert_binding_guarded(
                crate::runtime::runtime_binding_record(session_id, runtime)?,
                role,
            )
            .await;
        if result.is_err() {
            crate::clients::discard_unbound_runtime(runtime);
        }
        result
    }
}
