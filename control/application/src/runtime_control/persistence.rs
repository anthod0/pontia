use pontia_core::error::Result;
use pontia_runtime::RuntimeStartResult;
use pontia_storage_sqlite::repositories::{
    agent_bindings::SqliteAgentBindingRepository,
    runtime_bindings::{RuntimeBindingUpsertRecord, SqliteRuntimeBindingRepository},
};
use serde_json::json;

use super::RuntimeControlService;

#[derive(Debug, Clone)]
pub(super) struct TmuxPaneBinding {
    pub(super) socket_path: String,
    pub(super) pane_id: String,
}

impl RuntimeControlService {
    pub(super) async fn runtime_target(&self, session_id: &str) -> Result<Option<String>> {
        SqliteRuntimeBindingRepository::new(self.pool.clone())
            .runtime_handle(session_id)
            .await
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
        start_command: Option<&str>,
        session_id: &str,
        client_type: &str,
    ) -> Result<Option<String>> {
        let Some(command) = start_command else {
            return Ok(None);
        };
        let Some(session_identity_arg) = pontia_agent_clients::get_client_spec(client_type)
            .and_then(|spec| spec.tmux_runtime())
            .and_then(|runtime| runtime.resume_session_identity_arg)
        else {
            return Ok(Some(command.to_string()));
        };
        let Some(client_session_key) = self
            .client_session_key_for_session(session_id, client_type)
            .await?
        else {
            return Ok(Some(command.to_string()));
        };
        Ok(Some(format!(
            "{command} {session_identity_arg} {}",
            shell_quote(&client_session_key)
        )))
    }

    async fn client_session_key_for_session(
        &self,
        session_id: &str,
        client_type: &str,
    ) -> Result<Option<String>> {
        SqliteAgentBindingRepository::new(self.pool.clone())
            .client_session_key_for_session(session_id, client_type)
            .await
    }

    pub(super) async fn restart_count(&self, session_id: &str) -> Result<Option<i64>> {
        SqliteRuntimeBindingRepository::new(self.pool.clone())
            .restart_count(session_id)
            .await
    }

    pub(super) async fn upsert_runtime_binding(
        &self,
        session_id: &str,
        runtime: &RuntimeStartResult,
        persisted_start_command: Option<String>,
    ) -> Result<()> {
        let mut record = runtime_binding_record(session_id, runtime)?;
        record.start_command = persisted_start_command;
        let result = SqliteRuntimeBindingRepository::new(self.pool.clone())
            .upsert_binding_guarded(record)
            .await;
        if result.is_err() {
            let _ = self.runtime.terminate_session(&runtime.runtime_handle);
        }
        result
    }

    pub(super) async fn upsert_runtime_binding_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        session_id: &str,
        runtime: &RuntimeStartResult,
    ) -> Result<()> {
        let result = SqliteRuntimeBindingRepository::upsert_binding_in_tx(
            tx,
            runtime_binding_record(session_id, runtime)?,
        )
        .await;
        if result.is_err() {
            let _ = self.runtime.terminate_session(&runtime.runtime_handle);
        }
        result
    }
}

pub(crate) fn runtime_binding_record(
    session_id: &str,
    runtime: &RuntimeStartResult,
) -> Result<RuntimeBindingUpsertRecord> {
    let metadata = &runtime.metadata;
    let diagnostics = json!({
        "launch_id": metadata.get("launch_id"),
        "log_dir": metadata.get("log_dir"),
        "runtime_log": metadata.get("runtime_log"),
        "log_path": metadata.get("log_path"),
        "pi_hook_log": metadata.get("pi_hook_log"),
    });
    let adapter_details = json!({
        "tmux": metadata.get("tmux"),
        "in_process": metadata.get("in_process"),
    });
    Ok(RuntimeBindingUpsertRecord {
        session_id: session_id.to_string(),
        runtime_kind: runtime.runtime_kind.clone(),
        runtime_instance_id: runtime.runtime_instance_id().map(ToString::to_string),
        binding_state: if metadata["binding_confirmed"].as_bool() == Some(true) {
            "confirmed".to_string()
        } else {
            "provisioned".to_string()
        },
        runtime_handle: Some(runtime.runtime_handle.clone()),
        start_command: metadata["start_command"].as_str().map(ToString::to_string),
        launch_cwd: runtime.launch_cwd().map(ToString::to_string),
        internal_event_url: metadata["internal_event_url"]
            .as_str()
            .map(ToString::to_string),
        started_at: metadata["started_at"].as_str().map(ToString::to_string),
        last_seen_at: runtime.last_seen_at().map(ToString::to_string),
        restart_count: metadata["restart_count"].as_i64().unwrap_or(0),
        tmux_socket_path: runtime.tmux_socket_path().map(ToString::to_string),
        tmux_pane_id: runtime.tmux_pane_id().map(ToString::to_string),
        process_fingerprint: metadata
            .get("tmux_process_fingerprint")
            .filter(|value| !value.is_null())
            .map(serde_json::to_string)
            .transpose()?,
        capabilities: serde_json::to_string(&runtime.capabilities)?,
        diagnostics: serde_json::to_string(&diagnostics)?,
        adapter_details: serde_json::to_string(&adapter_details)?,
    })
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
