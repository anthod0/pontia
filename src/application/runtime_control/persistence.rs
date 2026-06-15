use super::*;

fn runtime_target_from_metadata(metadata: Value) -> Option<String> {
    metadata["tmux"]["session_name"]
        .as_str()
        .or_else(|| metadata["tmux_session"].as_str())
        .or_else(|| metadata["in_process"]["runtime_key"].as_str())
        .map(ToString::to_string)
}

#[derive(Debug, Clone)]
pub(super) struct TmuxPaneBinding {
    pub(super) socket_path: String,
    pub(super) pane_id: String,
}

impl RuntimeControlService {
    pub(super) async fn runtime_target(&self, session_id: &str) -> Result<Option<String>> {
        let metadata: Option<String> =
            sqlx::query_scalar("SELECT metadata FROM runtime_bindings WHERE session_id = ?")
                .bind(session_id)
                .fetch_optional(&self.pool)
                .await?;
        metadata
            .map(|metadata| {
                serde_json::from_str::<Value>(&metadata).map(runtime_target_from_metadata)
            })
            .transpose()
            .map_err(Into::into)
            .map(Option::flatten)
    }

    pub(super) async fn tmux_pane_binding(
        &self,
        session_id: &str,
    ) -> Result<Option<TmuxPaneBinding>> {
        let row = sqlx::query(
            "SELECT tmux_socket_path, tmux_pane_id FROM runtime_bindings WHERE session_id = ?",
        )
        .bind(session_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(|row| {
            let socket_path: Option<String> = row.try_get("tmux_socket_path")?;
            let pane_id: Option<String> = row.try_get("tmux_pane_id")?;
            Ok(match (socket_path, pane_id) {
                (Some(socket_path), Some(pane_id))
                    if !socket_path.trim().is_empty() && !pane_id.trim().is_empty() =>
                {
                    Some(TmuxPaneBinding {
                        socket_path,
                        pane_id,
                    })
                }
                _ => None,
            })
        })
        .transpose()
        .map(Option::flatten)
    }

    pub(super) async fn start_command(&self, session_id: &str) -> Result<Option<String>> {
        sqlx::query_scalar::<_, Option<String>>(
            "SELECT start_command FROM runtime_bindings WHERE session_id = ?",
        )
        .bind(session_id)
        .fetch_optional(&self.pool)
        .await
        .map(Option::flatten)
        .map_err(Into::into)
    }

    pub(super) async fn restart_count(&self, session_id: &str) -> Result<Option<i64>> {
        let metadata: Option<String> =
            sqlx::query_scalar("SELECT metadata FROM runtime_bindings WHERE session_id = ?")
                .bind(session_id)
                .fetch_optional(&self.pool)
                .await?;
        metadata
            .map(|metadata| {
                serde_json::from_str::<Value>(&metadata)
                    .map(|value| value["restart_count"].as_i64().unwrap_or(0))
            })
            .transpose()
            .map_err(Into::into)
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
}
