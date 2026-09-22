use pontia_agent_clients::AgentClientSpec;
use pontia_core::{
    error::{Error, Result},
    ids::new_runtime_instance_id,
};
use pontia_runtime::{GenericRuntimeManager, configured_internal_event_url, pontia_log_paths};
use pontia_storage_sqlite::repositories::runtime_bindings::{
    RuntimeBindingConfirmationRecord, SqliteRuntimeBindingRepository,
};
use serde_json::{Value, json};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use super::{
    RuntimeBindingUpsertRequest,
    metadata::{adapter_details, agent_binding_metadata, runtime_diagnostics},
    ownership::fence_runtime_binding_write,
    request::non_empty,
    service::RuntimeBindingUpsertService,
};
use crate::{
    AgentBindingService, ExternalQueryService, UpsertAgentBindingRequest, WorkspaceRecord,
};

impl RuntimeBindingUpsertService {
    pub(super) async fn confirm_binding(
        &self,
        session_id: &str,
        runtime_kind: &str,
        request: &RuntimeBindingUpsertRequest,
        workspace: &WorkspaceRecord,
        client_spec: &AgentClientSpec,
    ) -> Result<Value> {
        let log_paths = pontia_log_paths(&self.pontia_home);
        std::fs::create_dir_all(&log_paths.log_dir)?;
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_paths.runtime_log)?;
        let hook_log_metadata = client_spec
            .tmux_runtime()
            .and_then(|runtime| runtime.hook_log)
            .map(|hook_log| {
                (
                    hook_log.metadata_key,
                    log_paths.client_hook_log(hook_log.file_name),
                )
            });
        if let Some((_, hook_log_path)) = hook_log_metadata.as_ref() {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(hook_log_path)?;
        }
        let internal_event_url = configured_internal_event_url()
            .unwrap_or_else(|| "http://127.0.0.1:8080/internal/v1/events".to_string());
        let capabilities = client_spec.capabilities.clone();
        let last_seen_at = OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .map_err(|err| Error::Domain(format!("failed to format timestamp: {err}")))?;
        let tmux_socket_path = request
            .tmux
            .as_ref()
            .and_then(|tmux| non_empty(tmux.socket_path.as_deref()));
        let tmux_pane_id = request
            .tmux
            .as_ref()
            .and_then(|tmux| non_empty(tmux.pane_id.as_deref()));

        let hook_log_metadata_display = hook_log_metadata
            .as_ref()
            .map(|(metadata_key, path)| (*metadata_key, path.display().to_string()));
        let requested_runtime_instance_id = match non_empty(request.runtime_instance_id.as_deref())
        {
            Some(runtime_instance_id) => Some(runtime_instance_id),
            None => {
                self.unconfirmed_runtime_instance_id_for_pane(session_id, request)
                    .await?
            }
        };
        let process_fingerprint = if let (Some(socket_path), Some(pane_id), Some(tmux_runtime)) = (
            tmux_socket_path.as_deref(),
            tmux_pane_id.as_deref(),
            client_spec.tmux_runtime(),
        ) {
            match GenericRuntimeManager.capture_tmux_process_fingerprint(
                socket_path,
                pane_id,
                tmux_runtime.process_names,
            ) {
                Some(fingerprint) => Some(serde_json::to_string(&fingerprint)?),
                None => {
                    tracing::warn!(
                        session_id = %session_id,
                        client_type = %request.client_type,
                        pane_id,
                        "could not capture agent process fingerprint for tmux runtime binding"
                    );
                    None
                }
            }
        } else {
            None
        };
        let mut tx = self.pool.begin().await?;
        fence_runtime_binding_write(
            &mut tx,
            session_id,
            requested_runtime_instance_id.as_deref(),
        )
        .await?;
        let runtime_instance_id =
            requested_runtime_instance_id.unwrap_or_else(|| new_runtime_instance_id().to_string());
        let diagnostics = runtime_diagnostics(
            &log_paths.log_dir.display().to_string(),
            &log_paths.runtime_log.display().to_string(),
            hook_log_metadata_display
                .as_ref()
                .map(|(metadata_key, path)| (*metadata_key, path.as_str())),
        );
        SqliteRuntimeBindingRepository::confirm_binding_in_tx(
            &mut tx,
            RuntimeBindingConfirmationRecord {
                session_id: session_id.to_string(),
                runtime_kind: runtime_kind.to_string(),
                runtime_instance_id: runtime_instance_id.clone(),
                start_command: non_empty(request.start_command.as_deref()),
                launch_cwd: workspace.canonical_path.clone(),
                internal_event_url: internal_event_url.clone(),
                last_seen_at,
                tmux_socket_path: tmux_socket_path.clone(),
                tmux_pane_id: tmux_pane_id.clone(),
                process_fingerprint,
                capabilities: serde_json::to_string(&capabilities)?,
                diagnostics: serde_json::to_string(&diagnostics)?,
                adapter_details: serde_json::to_string(&adapter_details(request))?,
            },
        )
        .await?;
        tx.commit().await?;

        AgentBindingService::new(self.pool.clone())
            .upsert_binding(UpsertAgentBindingRequest {
                session_id: session_id.to_string(),
                client_type: request.client_type.clone(),
                launch_cwd: workspace.canonical_path.clone(),
                client_session_key: request.client_session_key.clone(),
                client_session_file: non_empty(request.client_session_file.as_deref()),
                metadata: agent_binding_metadata(request),
            })
            .await?;

        let session = ExternalQueryService::new(self.pool.clone())
            .get_session(session_id)
            .await?
            .ok_or_else(|| {
                Error::Domain(format!("session {session_id} missing after binding upsert"))
            })?;

        if let (Some(socket_path), Some(pane_id)) =
            (tmux_socket_path.as_deref(), tmux_pane_id.as_deref())
            && GenericRuntimeManager.is_tmux_pane_alive(socket_path, pane_id)
        {
            GenericRuntimeManager.mark_tmux_pane_for_session(
                socket_path,
                pane_id,
                session_id,
                &runtime_instance_id,
            )?;
        }

        Ok(json!({
            "session": session,
            "runtime": {
                "runtime_instance_id": runtime_instance_id,
                "internal_event_url": internal_event_url,
                "capabilities": capabilities,
            }
        }))
    }
}
