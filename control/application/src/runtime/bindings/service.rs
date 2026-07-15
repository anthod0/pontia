use serde_json::{Value, json};
use sqlx::SqlitePool;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use pontia_agent_clients as agent_clients;
use pontia_core::{
    domain::{EventSource, EventType, ReportedEvent},
    error::{Error, Result},
    ids::{new_event_id, new_session_id},
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
        agent_binding_metadata, binding_metadata, capabilities_for_tmux, is_fork_start, non_empty,
        validate_required,
    },
};
use crate::{
    AgentBindingService, EventIngestService, ExternalQueryService, UpsertAgentBindingRequest,
    WorkspaceRecord, upsert_workspace,
};

#[derive(Clone)]
pub struct RuntimeBindingUpsertService {
    pool: SqlitePool,
}

impl RuntimeBindingUpsertService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn upsert(&self, request: RuntimeBindingUpsertRequest) -> Result<Value> {
        validate_required("client_type", &request.client_type)?;
        validate_required("client_session_key", &request.client_session_key)?;
        validate_required("runtime_instance_id", &request.runtime_instance_id)?;
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

        let launch_cwd = request
            .launch_cwd
            .as_deref()
            .or(request.client_cwd.as_deref())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| Error::Domain("launch_cwd or client_cwd is required".to_string()))?;
        let workspace = upsert_workspace(&self.pool, launch_cwd).await?;

        let existing_session_id = self
            .session_id_for_client_session(&request.client_type, &request.client_session_key)
            .await?;
        let session_id = match existing_session_id {
            Some(session_id) => {
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
        let capabilities = capabilities_for_tmux(client_spec, request.tmux.as_ref());
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
        let metadata = binding_metadata(
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

        SqliteRuntimeBindingRepository::new(self.pool.clone())
            .upsert_binding(RuntimeBindingUpsertRecord {
                session_id: session_id.clone(),
                runtime_kind: runtime_kind.to_string(),
                runtime_instance_id: Some(request.runtime_instance_id.clone()),
                start_command: non_empty(request.start_command.as_deref()),
                launch_cwd: Some(workspace.canonical_path.clone()),
                last_seen_at: Some(last_seen_at.clone()),
                tmux_socket_path: tmux_socket_path.clone(),
                tmux_pane_id: tmux_pane_id.clone(),
                metadata: serde_json::to_string(&metadata)?,
            })
            .await?;

        if let (Some(socket_path), Some(pane_id)) =
            (tmux_socket_path.as_deref(), tmux_pane_id.as_deref())
        {
            let _ = GenericRuntimeManager.mark_tmux_pane_for_session(
                socket_path,
                pane_id,
                &session_id,
                &request.runtime_instance_id,
            );
        }

        AgentBindingService::new(self.pool.clone())
            .upsert_binding(UpsertAgentBindingRequest {
                session_id: session_id.clone(),
                client_type: request.client_type.clone(),
                launch_cwd: workspace.canonical_path.clone(),
                client_session_key: request.client_session_key.clone(),
                metadata: agent_binding_metadata(&request),
            })
            .await?;

        let session = ExternalQueryService::new(self.pool.clone())
            .get_session(&session_id)
            .await?
            .ok_or_else(|| {
                Error::Domain(format!("session {session_id} missing after binding upsert"))
            })?;

        Ok(json!({
            "session": session,
            "runtime": {
                "runtime_instance_id": request.runtime_instance_id,
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
                        .latest_client_session_key(&parent_session_id, &request.client_type)
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
            .ingest_event(ReportedEvent::new(
                new_event_id().to_string(),
                session_id.to_string(),
                None,
                EventSource::AgentClient,
                request.client_type.clone(),
                EventType::SessionResuming,
                json!({}),
            ))
            .await?;
        ingest
            .ingest_event(ReportedEvent::new(
                new_event_id().to_string(),
                session_id.to_string(),
                None,
                EventSource::AgentClient,
                request.client_type.clone(),
                EventType::SessionStarted,
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
            .ingest_event(ReportedEvent::new(
                new_event_id().to_string(),
                session_id.clone(),
                None,
                EventSource::AgentClient,
                request.client_type.clone(),
                EventType::SessionCreated,
                json!({
                    "workspace": workspace.canonical_path,
                    "metadata": {
                        "created_by": "runtime_binding_upsert",
                        "client_session_key": request.client_session_key,
                    }
                }),
            ))
            .await?;
        ingest
            .ingest_event(ReportedEvent::new(
                new_event_id().to_string(),
                session_id.clone(),
                None,
                EventSource::AgentClient,
                request.client_type.clone(),
                EventType::SessionStarting,
                json!({}),
            ))
            .await?;
        ingest
            .ingest_event(ReportedEvent::new(
                new_event_id().to_string(),
                session_id.clone(),
                None,
                EventSource::AgentClient,
                request.client_type.clone(),
                EventType::SessionStarted,
                json!({}),
            ))
            .await?;
        Ok(session_id)
    }
}
