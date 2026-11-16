use super::{
    RuntimeBindingUpsertRequest, metadata::agent_binding_metadata,
    ownership::fence_runtime_binding_write, request::non_empty,
    service::RuntimeBindingUpsertService,
};
use crate::client_contract::AgentClientSpec;
use crate::{
    ControlQueryService, PontiaEvent, PontiaEventSource, PontiaEventType,
    UpsertAgentBindingRequest, WorkspaceRecord,
};
use pontia_core::{Error, Result, ids::new_runtime_id, time::utc_now};
use pontia_storage_sqlite::repositories::session_runtimes::{
    SessionRuntimeRecord, SqliteSessionRuntimeRepository,
};
use serde_json::{Value, json};

impl RuntimeBindingUpsertService {
    pub(super) async fn confirm_binding(
        &self,
        session_id: &str,
        _runtime_kind: &str,
        request: &RuntimeBindingUpsertRequest,
        workspace: &WorkspaceRecord,
        client_spec: &AgentClientSpec,
        created_session: bool,
        process: &pontia_runtime::ProcessFingerprint,
    ) -> Result<Value> {
        let existing = SqliteSessionRuntimeRepository::new(self.pool.clone())
            .runtime_id(session_id, client_spec.adapter.control_runtime_role)
            .await?;
        let requested = non_empty(request.runtime_id.as_deref());
        if requested.is_some() && existing != requested {
            return Err(Error::StateConflict(
                "registration runtime does not belong to this Session".into(),
            ));
        }
        let runtime_needs_ready = existing.is_none() || created_session;
        let runtime_id = existing.unwrap_or_else(|| new_runtime_id().to_string());
        if runtime_needs_ready {
            self.events
                .ingest_pontia_event(PontiaEvent::new(
                    session_id,
                    None,
                    PontiaEventSource::RuntimeManager,
                    request.client_type.clone(),
                    PontiaEventType::RuntimeReady,
                    json!({"runtime_id": runtime_id.clone()}),
                ))
                .await?;
        }
        let prior = SqliteSessionRuntimeRepository::new(self.pool.clone())
            .get(&runtime_id)
            .await?;
        let fingerprint = serde_json::to_string(process)?;
        let terminal = prior.as_ref().filter(|record| {
            record
                .process_fingerprint
                .as_ref()
                .is_none_or(|existing| existing == &fingerprint)
        });
        let socket = terminal.and_then(|record| record.tmux_socket_path.clone());
        let pane = terminal.and_then(|record| record.tmux_pane_id.clone());
        let mut tx = self.pool.begin().await?;
        fence_runtime_binding_write(
            &mut tx,
            session_id,
            Some(&runtime_id),
            client_spec.adapter.control_runtime_role,
        )
        .await?;
        // Binding confirmation only adds identity and location. Runtime events own
        // every lifecycle transition, including an already-running attachment.
        let prior_state: String =
            sqlx::query_scalar("SELECT state FROM session_runtimes WHERE runtime_id = ?")
                .bind(&runtime_id)
                .fetch_one(&mut *tx)
                .await?;
        SqliteSessionRuntimeRepository::upsert_binding_in_tx(
            &mut tx,
            SessionRuntimeRecord {
                runtime_id: runtime_id.clone(),
                session_id: session_id.into(),
                role: "tui".into(),
                state: prior_state,
                start_command: non_empty(request.start_command.as_deref())
                    .or_else(|| prior.and_then(|record| record.start_command)),
                tmux_socket_path: socket,
                tmux_pane_id: pane,
                process_fingerprint: Some(fingerprint),
                created_at: utc_now()
                    .format(&time::format_description::well_known::Rfc3339)
                    .map_err(|err| Error::Domain(err.to_string()))?,
            },
        )
        .await?;
        crate::sessions::upsert_agent_binding_in_tx(
            &mut tx,
            UpsertAgentBindingRequest {
                session_id: session_id.into(),
                client_type: request.client_type.clone(),
                launch_cwd: workspace.canonical_path.clone(),
                client_session_key: request.client_session_key.clone(),
                client_session_file: non_empty(request.client_session_file.as_deref()),
                metadata: agent_binding_metadata(request),
            },
        )
        .await?;
        tx.commit().await?;
        let session = ControlQueryService::new(self.pool.clone())
            .with_clients(self.clients.clone())
            .get_session(session_id)
            .await?;
        Ok(
            json!({"session": session, "runtime": {"runtime_id": runtime_id, "capabilities": client_spec.capabilities}}),
        )
    }
}
