use super::*;
use pontia_storage_sqlite::repositories::{
    idempotency::SqliteIdempotencyRepository, sessions::SqliteSessionRepository,
};

impl SessionCommandService {
    pub(super) async fn idempotency_response(
        &self,
        operation: &str,
        key: &str,
    ) -> Result<Option<Value>> {
        SqliteIdempotencyRepository::new(self.pool.clone())
            .get_response(operation, key)
            .await
    }

    pub(super) async fn store_idempotency_response(
        &self,
        operation: &str,
        key: &str,
        response: &Value,
    ) -> Result<()> {
        SqliteIdempotencyRepository::new(self.pool.clone())
            .store_response(operation, key, response)
            .await
    }

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
        sqlx::query(
            r#"INSERT INTO runtime_bindings (
                   session_id,
                   runtime_kind,
                   runtime_instance_id,
                   start_command,
                   launch_cwd,
                   last_seen_at,
                   tmux_socket_path,
                   tmux_pane_id,
                   metadata
               )
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
               ON CONFLICT(session_id) DO UPDATE SET
                   runtime_kind = excluded.runtime_kind,
                   runtime_instance_id = excluded.runtime_instance_id,
                   start_command = excluded.start_command,
                   launch_cwd = excluded.launch_cwd,
                   last_seen_at = excluded.last_seen_at,
                   tmux_socket_path = excluded.tmux_socket_path,
                   tmux_pane_id = excluded.tmux_pane_id,
                   metadata = excluded.metadata,
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')"#,
        )
        .bind(session_id)
        .bind(&runtime.runtime_kind)
        .bind(runtime.runtime_instance_id())
        .bind(runtime.metadata["start_command"].as_str())
        .bind(runtime.launch_cwd())
        .bind(runtime.last_seen_at())
        .bind(runtime.tmux_socket_path())
        .bind(runtime.tmux_pane_id())
        .bind(serde_json::to_string(&runtime.binding_metadata())?)
        .execute(&self.pool)
        .await?;
        Ok(())
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
