use pontia_core::error::Result;
use pontia_runtime::RuntimeStartResult;
use pontia_storage_sqlite::repositories::runtime_bindings::{
    RuntimeBindingUpsertRecord, SqliteRuntimeBindingRepository,
};
use serde_json::json;

use super::SessionCommandService;

impl SessionCommandService {
    pub(super) async fn start_command(&self, session_id: &str) -> Result<Option<String>> {
        SqliteRuntimeBindingRepository::new(self.pool.clone())
            .start_command(session_id)
            .await
    }

    pub(super) async fn restart_count(&self, session_id: &str) -> Result<Option<i64>> {
        SqliteRuntimeBindingRepository::new(self.pool.clone())
            .restart_count(session_id)
            .await
    }

    pub(super) async fn upsert_resumed_runtime_binding(
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
            crate::clients::discard_unbound_runtime(runtime);
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
            crate::clients::discard_unbound_runtime(runtime);
        }
        result
    }
}

pub(crate) fn runtime_binding_record(
    session_id: &str,
    runtime: &RuntimeStartResult,
) -> Result<RuntimeBindingUpsertRecord> {
    let metadata = &runtime.metadata;
    let mut diagnostics = json!({
        "launch_id": metadata.get("launch_id"),
        "log_dir": metadata.get("log_dir"),
        "runtime_log": metadata.get("runtime_log"),
        "log_path": metadata.get("log_path"),
    });
    if let Some(client) = metadata
        .get("client_diagnostics")
        .and_then(serde_json::Value::as_object)
    {
        diagnostics
            .as_object_mut()
            .expect("diagnostics object")
            .extend(client.clone());
    }
    let adapter_details = json!({
        "codex": metadata.get("codex"),
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
