use super::*;
use pontia_storage_sqlite::repositories::{
    runtime_bindings::{RuntimeBindingUpsertRecord, SqliteRuntimeBindingRepository},
    sessions::SqliteSessionRepository,
};

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
            .upsert_binding_guarded(RuntimeBindingUpsertRecord {
                session_id: session_id.to_string(),
                runtime_kind: runtime.runtime_kind.clone(),
                runtime_instance_id: runtime.runtime_instance_id().map(ToString::to_string),
                start_command: runtime.metadata["start_command"]
                    .as_str()
                    .map(ToString::to_string),
                launch_cwd: runtime.launch_cwd().map(ToString::to_string),
                last_seen_at: runtime.last_seen_at().map(ToString::to_string),
                tmux_socket_path: runtime.tmux_socket_path().map(ToString::to_string),
                tmux_pane_id: runtime.tmux_pane_id().map(ToString::to_string),
                metadata: serde_json::to_string(&runtime.binding_metadata())?,
            })
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
