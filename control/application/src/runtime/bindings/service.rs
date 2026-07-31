use serde_json::{Value, json};
use sqlx::SqlitePool;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use tokio::sync::Mutex;

use pontia_agent_clients as agent_clients;
use pontia_core::{
    error::{Error, Result},
    ids::{new_runtime_instance_id, new_session_id},
};
use pontia_runtime::{GenericRuntimeManager, configured_internal_event_url, pontia_log_paths};
use pontia_storage_sqlite::repositories::{
    agent_bindings::SqliteAgentBindingRepository,
    runtime_bindings::{RuntimeBindingUpsertRecord, SqliteRuntimeBindingRepository},
    sessions::SqliteSessionRepository,
};

use super::{
    RuntimeBindingUpsertRequest,
    helpers::{
        agent_binding_metadata, binding_metadata, is_fork_start, non_empty, validate_required,
    },
};
use crate::{
    AgentBindingService, EventIngestService, ExternalQueryService, PontiaEvent, PontiaEventSource,
    PontiaEventType, UpsertAgentBindingRequest, WorkspaceRecord, upsert_workspace,
};

static RUNTIME_BINDING_UPSERT_LOCK: Mutex<()> = Mutex::const_new(());

#[derive(Clone)]
pub struct RuntimeBindingUpsertService {
    pool: SqlitePool,
}

impl RuntimeBindingUpsertService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn upsert(&self, request: RuntimeBindingUpsertRequest) -> Result<Value> {
        let _upsert_guard = RUNTIME_BINDING_UPSERT_LOCK.lock().await;
        validate_required("client_type", &request.client_type)?;
        validate_required("client_session_key", &request.client_session_key)?;
        let client_spec =
            agent_clients::get_client_spec(&request.client_type).ok_or_else(|| {
                Error::Domain(format!("unsupported client_type: {}", request.client_type))
            })?;
        let runtime_kind = client_spec.runtime_binding_kind().ok_or_else(|| {
            Error::Domain(format!(
                "runtime binding upsert does not support client_type {}",
                request.client_type
            ))
        })?;
        let tmux = request
            .tmux
            .as_ref()
            .ok_or_else(|| Error::Domain("runtime binding upsert requires tmux".to_string()))?;
        if non_empty(tmux.socket_path.as_deref()).is_none()
            || non_empty(tmux.pane_id.as_deref()).is_none()
        {
            return Err(Error::Domain(
                "runtime binding upsert requires tmux.socket_path and tmux.pane_id".to_string(),
            ));
        }

        let launch_cwd = request
            .launch_cwd
            .as_deref()
            .or(request.client_cwd.as_deref())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| Error::Domain("launch_cwd or client_cwd is required".to_string()))?;
        let workspace = upsert_workspace(&self.pool, launch_cwd).await?;

        let existing_session_id = if let Some(session_id) = non_empty(request.session_id.as_deref())
        {
            self.ensure_requested_session(&session_id, &request).await?;
            Some(session_id)
        } else {
            match self
                .session_id_for_client_session(&request.client_type, &request.client_session_key)
                .await?
            {
                Some(session_id) => Some(session_id),
                None => self.unbound_session_id_for_client_session(&request).await?,
            }
        };
        let session_id = match existing_session_id {
            Some(session_id) => {
                self.ensure_existing_binding_agrees(&session_id, &request)
                    .await?;
                self.ensure_active_runtime_is_not_replaced(&session_id, &request)
                    .await?;
                self.record_resume_lifecycle_for_exited_session(&session_id, &request)
                    .await?;
                session_id
            }
            None => self.create_bound_session(&request, &workspace).await?,
        };

        if is_fork_start(&request) {
            self.upsert_fork_lineage(&session_id, &request).await?;
        }

        SqliteSessionRepository::new(self.pool.clone())
            .update_session_workspace(
                &session_id,
                Some(&workspace.canonical_path),
                Some(&workspace.workspace_id),
            )
            .await?;

        let log_paths = pontia_log_paths()?;
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
                self.unconfirmed_runtime_instance_id_for_pane(&session_id, &request)
                    .await?
            }
        };
        let mut confirmed_metadata = binding_metadata(
            &request,
            &workspace.canonical_path,
            &internal_event_url,
            &log_paths.log_dir.display().to_string(),
            &log_paths.runtime_log.display().to_string(),
            hook_log_metadata_display
                .as_ref()
                .map(|(metadata_key, path)| (*metadata_key, path.as_str())),
            &capabilities,
        );
        let mut tx = self.pool.begin().await?;
        pontia_storage_sqlite::repositories::turns::SqliteTurnRepository::serialize_session_turn_writes_in_tx(
            &mut tx,
            &session_id,
        )
        .await?;
        SqliteRuntimeBindingRepository::ensure_runtime_owner_may_write_in_tx(
            &mut tx,
            &session_id,
            requested_runtime_instance_id.as_deref(),
        )
        .await?;
        let runtime_instance_id =
            requested_runtime_instance_id.unwrap_or_else(|| new_runtime_instance_id().to_string());
        confirmed_metadata["runtime_instance_id"] = json!(runtime_instance_id);
        confirmed_metadata["binding_confirmed"] = json!(true);
        let mut metadata = SqliteRuntimeBindingRepository::metadata_in_tx(&mut tx, &session_id)
            .await?
            .map(|metadata| serde_json::from_str::<Value>(&metadata))
            .transpose()?
            .unwrap_or_else(|| json!({}));
        if let (Some(existing), Some(confirmed)) =
            (metadata.as_object_mut(), confirmed_metadata.as_object())
        {
            existing.extend(confirmed.clone());
        } else {
            metadata = confirmed_metadata;
        }

        SqliteRuntimeBindingRepository::upsert_binding_in_tx(
            &mut tx,
            RuntimeBindingUpsertRecord {
                session_id: session_id.clone(),
                runtime_kind: runtime_kind.to_string(),
                runtime_instance_id: Some(runtime_instance_id.clone()),
                start_command: non_empty(request.start_command.as_deref()),
                launch_cwd: Some(workspace.canonical_path.clone()),
                last_seen_at: Some(last_seen_at.clone()),
                tmux_socket_path: tmux_socket_path.clone(),
                tmux_pane_id: tmux_pane_id.clone(),
                metadata: serde_json::to_string(&metadata)?,
            },
        )
        .await?;
        tx.commit().await?;

        AgentBindingService::new(self.pool.clone())
            .upsert_binding(UpsertAgentBindingRequest {
                session_id: session_id.clone(),
                client_type: request.client_type.clone(),
                launch_cwd: workspace.canonical_path.clone(),
                client_session_key: request.client_session_key.clone(),
                client_session_file: non_empty(request.client_session_file.as_deref()),
                metadata: agent_binding_metadata(&request),
            })
            .await?;

        let session = ExternalQueryService::new(self.pool.clone())
            .get_session(&session_id)
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
                &session_id,
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

    async fn session_id_for_client_session(
        &self,
        client_type: &str,
        client_session_key: &str,
    ) -> Result<Option<String>> {
        SqliteAgentBindingRepository::new(self.pool.clone())
            .session_id_for_client_session(client_type, client_session_key)
            .await
    }

    async fn unbound_session_id_for_client_session(
        &self,
        request: &RuntimeBindingUpsertRequest,
    ) -> Result<Option<String>> {
        if request.client_type == "pi"
            && let Some(session_id) = sqlx::query_scalar(
                r#"SELECT s.session_id
                   FROM sessions s
                   LEFT JOIN agent_bindings a ON a.session_id = s.session_id
                   WHERE s.session_id = ? AND s.client_type = ? AND a.id IS NULL"#,
            )
            .bind(&request.client_session_key)
            .bind(&request.client_type)
            .fetch_optional(&self.pool)
            .await?
        {
            return Ok(Some(session_id));
        }

        let Some(tmux) = request.tmux.as_ref() else {
            return Ok(None);
        };
        let Some(socket_path) = non_empty(tmux.socket_path.as_deref()) else {
            return Ok(None);
        };
        let Some(pane_id) = non_empty(tmux.pane_id.as_deref()) else {
            return Ok(None);
        };
        Ok(sqlx::query_scalar(
            r#"SELECT s.session_id
               FROM sessions s
               JOIN runtime_bindings r ON r.session_id = s.session_id
               LEFT JOIN agent_bindings a ON a.session_id = s.session_id
               WHERE s.client_type = ?
                 AND s.state != 'exited'
                 AND a.id IS NULL
                 AND r.tmux_socket_path = ?
                 AND r.tmux_pane_id = ?
                 AND COALESCE(json_extract(r.metadata, '$.binding_confirmed'), 0) = 0"#,
        )
        .bind(&request.client_type)
        .bind(socket_path)
        .bind(pane_id)
        .fetch_optional(&self.pool)
        .await?)
    }

    async fn unconfirmed_runtime_instance_id_for_pane(
        &self,
        session_id: &str,
        request: &RuntimeBindingUpsertRequest,
    ) -> Result<Option<String>> {
        let Some(tmux) = request.tmux.as_ref() else {
            return Ok(None);
        };
        let Some(socket_path) = non_empty(tmux.socket_path.as_deref()) else {
            return Ok(None);
        };
        let Some(pane_id) = non_empty(tmux.pane_id.as_deref()) else {
            return Ok(None);
        };
        Ok(sqlx::query_scalar(
            r#"SELECT runtime_instance_id
               FROM runtime_bindings
               WHERE session_id = ?
                 AND tmux_socket_path = ?
                 AND tmux_pane_id = ?
                 AND COALESCE(json_extract(metadata, '$.binding_confirmed'), 0) = 0"#,
        )
        .bind(session_id)
        .bind(socket_path)
        .bind(pane_id)
        .fetch_optional(&self.pool)
        .await?
        .flatten())
    }

    async fn ensure_requested_session(
        &self,
        session_id: &str,
        request: &RuntimeBindingUpsertRequest,
    ) -> Result<()> {
        let session = SqliteSessionRepository::new(self.pool.clone())
            .get_session(session_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("session {session_id} not found")))?;
        if session.client_type != request.client_type {
            return Err(Error::StateConflict(format!(
                "session {session_id} uses client_type {}, not {}",
                session.client_type, request.client_type
            )));
        }
        if let Some(owner) = AgentBindingService::new(self.pool.clone())
            .binding_for_client_session(&request.client_type, &request.client_session_key)
            .await?
            && owner.session_id != session_id
        {
            return Err(Error::StateConflict(format!(
                "client session identity is already bound to session {}",
                owner.session_id
            )));
        }
        Ok(())
    }

    async fn ensure_existing_binding_agrees(
        &self,
        session_id: &str,
        request: &RuntimeBindingUpsertRequest,
    ) -> Result<()> {
        if let Some(binding) = AgentBindingService::new(self.pool.clone())
            .binding_for_client_session(&request.client_type, &request.client_session_key)
            .await?
            && binding.session_id != session_id
        {
            return Err(Error::StateConflict(format!(
                "runtime binding update does not match session {session_id} Agent binding"
            )));
        }
        Ok(())
    }

    async fn ensure_active_runtime_is_not_replaced(
        &self,
        session_id: &str,
        request: &RuntimeBindingUpsertRequest,
    ) -> Result<()> {
        let state: String = sqlx::query_scalar("SELECT state FROM sessions WHERE session_id = ?")
            .bind(session_id)
            .fetch_one(&self.pool)
            .await?;
        if state == "exited" {
            return Ok(());
        }

        let Some(binding) = AgentBindingService::new(self.pool.clone())
            .binding_for_session(session_id)
            .await?
        else {
            // The Control Plane may have created the runtime before the TUI has
            // confirmed its native client identity for the first time.
            return Ok(());
        };
        let existing_runtime_instance_id = SqliteRuntimeBindingRepository::new(self.pool.clone())
            .runtime_instance_id(session_id)
            .await?;
        let existing_tmux = SqliteRuntimeBindingRepository::new(self.pool.clone())
            .tmux_pane_binding(session_id)
            .await?;
        let incoming_tmux = request.tmux.as_ref().and_then(|tmux| {
            Some((
                non_empty(tmux.socket_path.as_deref())?,
                non_empty(tmux.pane_id.as_deref())?,
            ))
        });
        let same_runtime = non_empty(request.runtime_instance_id.as_deref())
            .zip(existing_runtime_instance_id.as_deref())
            .is_some_and(|(incoming, existing)| incoming == existing);
        let same_pane = match (existing_tmux, incoming_tmux) {
            (Some(existing), Some((incoming_socket, incoming_pane))) => {
                existing.socket_path.as_deref() == Some(incoming_socket.as_str())
                    && existing.pane_id.as_deref() == Some(incoming_pane.as_str())
            }
            (None, None) => true,
            _ => false,
        };
        let same_client = binding.client_type == request.client_type
            && binding.client_session_key == request.client_session_key;

        if same_client && same_runtime && same_pane {
            return Ok(());
        }
        Err(Error::StateConflict(format!(
            "session {session_id} already has an active Pontia-managed agent TUI"
        )))
    }

    async fn upsert_fork_lineage(
        &self,
        child_session_id: &str,
        request: &RuntimeBindingUpsertRequest,
    ) -> Result<()> {
        let parent_session_id = self.resolve_parent_session_id(request).await?.ok_or_else(|| {
            Error::Domain(
                "fork runtime binding upsert requires parent_session_id or parent_client_session_key"
                    .to_string(),
            )
        })?;
        if parent_session_id == child_session_id {
            return Err(Error::Domain(
                "fork child session cannot be the same as parent session".to_string(),
            ));
        }
        if !SqliteSessionRepository::new(self.pool.clone())
            .exists(&parent_session_id)
            .await?
        {
            return Err(Error::NotFound(format!(
                "parent session {parent_session_id} not found"
            )));
        }
        let parent_client_session_key =
            match non_empty(request.parent_client_session_key.as_deref()) {
                Some(key) => Some(key),
                None => {
                    SqliteAgentBindingRepository::new(self.pool.clone())
                        .client_session_key_for_session(&parent_session_id, &request.client_type)
                        .await?
                }
            };
        let metadata = if request.lineage_metadata.is_null() {
            json!({})
        } else {
            request.lineage_metadata.clone()
        };
        sqlx::query(
            r#"INSERT INTO session_lineage
               (child_session_id, parent_session_id, relation_type, forked_from_turn_id,
                forked_from_client_node_id, parent_client_session_key, child_client_session_key,
                metadata)
               VALUES (?, ?, 'fork', ?, ?, ?, ?, ?)
               ON CONFLICT(child_session_id) DO UPDATE SET
                   parent_session_id = excluded.parent_session_id,
                   relation_type = excluded.relation_type,
                   forked_from_turn_id = excluded.forked_from_turn_id,
                   forked_from_client_node_id = excluded.forked_from_client_node_id,
                   parent_client_session_key = excluded.parent_client_session_key,
                   child_client_session_key = excluded.child_client_session_key,
                   metadata = excluded.metadata"#,
        )
        .bind(child_session_id)
        .bind(parent_session_id)
        .bind(non_empty(request.forked_from_turn_id.as_deref()))
        .bind(non_empty(request.forked_from_client_node_id.as_deref()))
        .bind(parent_client_session_key)
        .bind(non_empty(Some(&request.client_session_key)))
        .bind(serde_json::to_string(&metadata)?)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn resolve_parent_session_id(
        &self,
        request: &RuntimeBindingUpsertRequest,
    ) -> Result<Option<String>> {
        if let Some(parent_session_id) = non_empty(request.parent_session_id.as_deref()) {
            return Ok(Some(parent_session_id));
        }
        if let Some(parent_client_session_key) =
            non_empty(request.parent_client_session_key.as_deref())
        {
            return self
                .session_id_for_client_session(&request.client_type, &parent_client_session_key)
                .await;
        }
        Ok(None)
    }

    async fn record_resume_lifecycle_for_exited_session(
        &self,
        session_id: &str,
        request: &RuntimeBindingUpsertRequest,
    ) -> Result<()> {
        let state: Option<String> =
            sqlx::query_scalar("SELECT state FROM sessions WHERE session_id = ?")
                .bind(session_id)
                .fetch_optional(&self.pool)
                .await?;
        if state.as_deref() != Some("exited") {
            return Ok(());
        }

        let ingest = EventIngestService::new(self.pool.clone());
        ingest
            .ingest_pontia_event(PontiaEvent::new(
                session_id.to_string(),
                None,
                PontiaEventSource::RuntimeManager,
                request.client_type.clone(),
                PontiaEventType::SessionResuming,
                json!({}),
            ))
            .await?;
        ingest
            .ingest_pontia_event(PontiaEvent::new(
                session_id.to_string(),
                None,
                PontiaEventSource::RuntimeManager,
                request.client_type.clone(),
                PontiaEventType::SessionStarted,
                json!({}),
            ))
            .await?;
        Ok(())
    }

    async fn create_bound_session(
        &self,
        request: &RuntimeBindingUpsertRequest,
        workspace: &WorkspaceRecord,
    ) -> Result<String> {
        let session_id = new_session_id().to_string();
        let ingest = EventIngestService::new(self.pool.clone());
        ingest
            .ingest_pontia_event_with_agent_binding(
                PontiaEvent::new(
                    session_id.clone(),
                    None,
                    PontiaEventSource::RuntimeManager,
                    request.client_type.clone(),
                    PontiaEventType::SessionCreated,
                    json!({
                        "workspace": workspace.canonical_path,
                        "metadata": {
                            "created_by": "runtime_binding_upsert",
                            "client_session_key": request.client_session_key,
                        }
                    }),
                ),
                UpsertAgentBindingRequest {
                    session_id: session_id.clone(),
                    client_type: request.client_type.clone(),
                    launch_cwd: workspace.canonical_path.clone(),
                    client_session_key: request.client_session_key.clone(),
                    client_session_file: non_empty(request.client_session_file.as_deref()),
                    metadata: agent_binding_metadata(request),
                },
            )
            .await?;
        ingest
            .ingest_pontia_event(PontiaEvent::new(
                session_id.clone(),
                None,
                PontiaEventSource::RuntimeManager,
                request.client_type.clone(),
                PontiaEventType::SessionStarting,
                json!({}),
            ))
            .await?;
        ingest
            .ingest_pontia_event(PontiaEvent::new(
                session_id.clone(),
                None,
                PontiaEventSource::RuntimeManager,
                request.client_type.clone(),
                PontiaEventType::SessionStarted,
                json!({}),
            ))
            .await?;
        Ok(session_id)
    }
}
