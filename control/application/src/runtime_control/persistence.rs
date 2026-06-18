use super::*;
use pontia_storage_sqlite::repositories::{
    agent_bindings::SqliteAgentBindingRepository,
    runtime_bindings::{RuntimeBindingUpsertRecord, SqliteRuntimeBindingRepository},
};

fn runtime_target_from_metadata(metadata: Value) -> Option<String> {
    metadata["in_process"]["runtime_handle"]
        .as_str()
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
        let metadata = SqliteRuntimeBindingRepository::new(self.pool.clone())
            .metadata(session_id)
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
        SqliteRuntimeBindingRepository::new(self.pool.clone())
            .tmux_pane_binding(session_id)
            .await?
            .map(|row| match (row.socket_path, row.pane_id) {
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
            .map(Ok)
            .transpose()
            .map(Option::flatten)
    }

    pub(super) async fn start_command(&self, session_id: &str) -> Result<Option<String>> {
        SqliteRuntimeBindingRepository::new(self.pool.clone())
            .start_command(session_id)
            .await
    }

    pub(super) async fn resume_start_command(
        &self,
        session_id: &str,
        client_type: &str,
    ) -> Result<Option<String>> {
        let Some(command) = self.start_command(session_id).await? else {
            return Ok(None);
        };
        let Some(session_identity_arg) = pontia_agent_clients::get_client_spec(client_type)
            .and_then(|spec| spec.tmux_runtime())
            .and_then(|runtime| runtime.session_identity_arg)
        else {
            return Ok(Some(command));
        };
        let Some(client_session_key) = self
            .latest_client_session_key(session_id, client_type)
            .await?
        else {
            return Ok(Some(command));
        };
        Ok(Some(format!(
            "{command} {session_identity_arg} {}",
            shell_quote(&client_session_key)
        )))
    }

    async fn latest_client_session_key(
        &self,
        session_id: &str,
        client_type: &str,
    ) -> Result<Option<String>> {
        SqliteAgentBindingRepository::new(self.pool.clone())
            .latest_client_session_key(session_id, client_type)
            .await
    }

    pub(super) async fn restart_count(&self, session_id: &str) -> Result<Option<i64>> {
        let metadata = SqliteRuntimeBindingRepository::new(self.pool.clone())
            .metadata(session_id)
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
        SqliteRuntimeBindingRepository::new(self.pool.clone())
            .upsert_binding(RuntimeBindingUpsertRecord {
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
            .await
    }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
