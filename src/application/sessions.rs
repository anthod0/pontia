use super::turns::write_client_current_turn_context;
use super::*;
use crate::agent_clients::{DispatchMode, ReadinessMode, get_client_spec};

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct CreateSessionRequest {
    #[serde(default = "default_client_type")]
    pub client_type: String,
    pub workspace: Option<String>,
    pub workspace_id: Option<String>,
    pub handle: Option<String>,
    pub role: Option<String>,
    pub description: Option<String>,
    pub execution_profile_id: Option<String>,
    pub execution_profile_version: Option<String>,
    #[serde(default)]
    pub metadata: Value,
    pub initial_task: Option<InitialTaskRequest>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct InitialTaskRequest {
    pub input: String,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CreateSessionOutcome {
    pub data: Value,
    pub duplicate: bool,
}

fn client_dispatch_mode(client_type: &str) -> Result<DispatchMode> {
    get_client_spec(client_type)
        .map(|spec| spec.dispatch_mode)
        .ok_or_else(|| Error::Domain(format!("unsupported client_type: {client_type}")))
}

fn client_readiness_mode(client_type: &str) -> Result<ReadinessMode> {
    get_client_spec(client_type)
        .map(|spec| spec.readiness_mode)
        .ok_or_else(|| Error::Domain(format!("unsupported client_type: {client_type}")))
}

fn validate_handle(handle: &str) -> Result<()> {
    let mut chars = handle.chars();
    if chars.next() != Some('@') {
        return Err(invalid_handle(handle));
    }
    let Some(first) = chars.next() else {
        return Err(invalid_handle(handle));
    };
    if !first.is_ascii_lowercase() {
        return Err(invalid_handle(handle));
    }
    let remaining: Vec<char> = chars.collect();
    if remaining.is_empty() || remaining.len() > 30 {
        return Err(invalid_handle(handle));
    }
    if !remaining
        .iter()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || *ch == '_' || *ch == '-')
    {
        return Err(invalid_handle(handle));
    }
    Ok(())
}

fn invalid_handle(handle: &str) -> Error {
    Error::Domain(format!(
        "Invalid session handle {handle}. Handle must match @[a-z][a-z0-9_-]{{1,31}}."
    ))
}

#[derive(Clone)]
pub struct SessionCommandService {
    pool: SqlitePool,
    runtime: GenericRuntimeManager,
}

impl SessionCommandService {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            runtime: GenericRuntimeManager,
        }
    }

    pub async fn create_session(
        &self,
        request: CreateSessionRequest,
        idempotency_key: Option<&str>,
    ) -> Result<CreateSessionOutcome> {
        if !is_supported_client_type(&request.client_type) {
            return Err(crate::error::Error::Domain(format!(
                "unsupported client_type: {}",
                request.client_type
            )));
        }

        if let Some(key) = idempotency_key
            && let Some(response) = self.idempotency_response("create_session", key).await?
        {
            return Ok(CreateSessionOutcome {
                data: response,
                duplicate: true,
            });
        }

        let handle = request.handle.as_deref();
        if let Some(handle) = handle {
            validate_handle(handle)?;
        }
        if request.workspace.is_some() && request.workspace_id.is_some() {
            return Err(Error::Domain(
                "workspace and workspace_id cannot both be provided".to_string(),
            ));
        }
        if let Some(handle) = handle
            && request.workspace.is_none()
            && request.workspace_id.is_none()
        {
            return Err(Error::Domain(format!(
                "Cannot create session with handle {handle} because workspace is required."
            )));
        }

        let workspace_record = if let Some(workspace_id) = request.workspace_id.as_deref() {
            Some(
                get_workspace_record(&self.pool, workspace_id)
                    .await?
                    .ok_or_else(|| {
                        Error::NotFound(format!("workspace {workspace_id} not found"))
                    })?,
            )
        } else if let Some(workspace) = request.workspace.as_deref() {
            Some(upsert_workspace(&self.pool, workspace).await?)
        } else {
            None
        };
        if let (Some(workspace), Some(handle)) = (workspace_record.as_ref(), handle) {
            self.ensure_handle_available(&workspace.workspace_id, handle)
                .await?;
        }
        let runtime_workspace = workspace_record
            .as_ref()
            .map(|workspace| workspace.canonical_path.clone());

        let session_id = new_session_id().to_string();
        let ingest = EventIngestService::new(self.pool.clone());

        ingest
            .ingest_event(DomainEvent::new(
                new_event_id().to_string(),
                session_id.clone(),
                None,
                EventSource::ExternalApi,
                request.client_type.clone(),
                EventType::SessionCreated,
                json!({
                    "workspace": runtime_workspace,
                    "handle": request.handle,
                    "role": request.role,
                    "description": request.description,
                    "execution_profile_id": request.execution_profile_id,
                    "execution_profile_version": request.execution_profile_version,
                    "metadata": request.metadata,
                }),
            ))
            .await?;
        ingest
            .ingest_event(DomainEvent::new(
                new_event_id().to_string(),
                session_id.clone(),
                None,
                EventSource::ExternalApi,
                request.client_type.clone(),
                EventType::SessionStarting,
                json!({}),
            ))
            .await?;

        let runtime = self.runtime.start_session(RuntimeStartRequest {
            session_id: session_id.clone(),
            client_type: request.client_type.clone(),
            workspace: runtime_workspace.clone(),
            handle: request.handle.clone(),
            role: request.role.clone(),
        })?;
        self.upsert_runtime_binding(&session_id, &runtime).await?;
        self.update_session_workspace(&session_id, workspace_record.as_ref())
            .await?;

        ingest
            .ingest_event(DomainEvent::new(
                new_event_id().to_string(),
                session_id.clone(),
                None,
                EventSource::RuntimeManager,
                request.client_type.clone(),
                EventType::SessionStarted,
                json!({}),
            ))
            .await?;
        if client_readiness_mode(&request.client_type)? == ReadinessMode::RuntimeManagerImmediate {
            ingest
                .ingest_event(DomainEvent::new(
                    new_event_id().to_string(),
                    session_id.clone(),
                    None,
                    EventSource::RuntimeManager,
                    request.client_type.clone(),
                    EventType::SessionReady,
                    json!({}),
                ))
                .await?;
        }

        let initial_turn_id = if let Some(initial_task) = request.initial_task {
            let turn_id = new_turn_id().to_string();
            ingest
                .ingest_event(DomainEvent::new(
                    new_event_id().to_string(),
                    session_id.clone(),
                    Some(turn_id.clone()),
                    EventSource::ExternalApi,
                    request.client_type.clone(),
                    EventType::TurnCreated,
                    json!({
                        "input": { "summary": initial_task.input },
                        "metadata": initial_task.metadata,
                    }),
                ))
                .await?;
            ingest
                .ingest_event(DomainEvent::new(
                    new_event_id().to_string(),
                    session_id.clone(),
                    Some(turn_id.clone()),
                    EventSource::ExternalApi,
                    request.client_type.clone(),
                    EventType::TurnQueued,
                    json!({}),
                ))
                .await?;
            if client_dispatch_mode(&request.client_type)? == DispatchMode::TmuxPaste {
                self.wait_and_dispatch_initial_tui_turn(
                    &session_id,
                    &turn_id,
                    &request.client_type,
                    &initial_task.input,
                    &runtime,
                )
                .await?;
            }
            Some(turn_id)
        } else {
            None
        };

        let query = ExternalQueryService::new(self.pool.clone());
        let session = query
            .get_session(&session_id)
            .await?
            .ok_or_else(|| crate::error::Error::Domain("created session missing".to_string()))?;
        let initial_turn = if let Some(turn_id) = initial_turn_id {
            query.get_turn(&session_id, &turn_id).await?
        } else {
            None
        };
        let data = json!({ "session": session, "initial_turn": initial_turn });

        if let Some(key) = idempotency_key {
            self.store_idempotency_response("create_session", key, &data)
                .await?;
        }

        Ok(CreateSessionOutcome {
            data,
            duplicate: false,
        })
    }

    async fn wait_and_dispatch_initial_tui_turn(
        &self,
        session_id: &str,
        turn_id: &str,
        client_type: &str,
        input: &str,
        runtime: &RuntimeStartResult,
    ) -> Result<()> {
        let runtime_instance_id = runtime.metadata["runtime_instance_id"]
            .as_str()
            .ok_or_else(|| {
                Error::Domain(format!(
                    "{client_type} runtime metadata missing runtime_instance_id"
                ))
            })?;
        let agent_input = AgentInput {
            session_id: session_id.to_string(),
            turn_id: turn_id.to_string(),
            input: input.to_string(),
        };
        let ingest = EventIngestService::new(self.pool.clone());
        let dispatch_result = RuntimeReadinessService::new(self.pool.clone())
            .wait_until_ready(session_id, client_type, runtime_instance_id)
            .await
            .and_then(|()| {
                write_client_current_turn_context(&runtime.metadata, &agent_input, client_type)
            })
            .and_then(|()| {
                self.runtime
                    .dispatch_tui_turn(&runtime.runtime_ref, client_type, &agent_input)
            });

        match dispatch_result {
            Ok(()) => {
                ingest
                    .ingest_event(DomainEvent::new(
                        new_event_id().to_string(),
                        session_id.to_string(),
                        Some(turn_id.to_string()),
                        EventSource::AgentAdapter,
                        client_type.to_string(),
                        EventType::TurnStarted,
                        json!({}),
                    ))
                    .await?;
            }
            Err(error) => {
                ingest
                    .ingest_event(DomainEvent::new(
                        new_event_id().to_string(),
                        session_id.to_string(),
                        Some(turn_id.to_string()),
                        EventSource::RuntimeManager,
                        client_type.to_string(),
                        EventType::TurnFailed,
                        json!({ "failure": { "message": error.to_string() } }),
                    ))
                    .await?;
            }
        }
        Ok(())
    }

    async fn idempotency_response(&self, operation: &str, key: &str) -> Result<Option<Value>> {
        let response: Option<String> = sqlx::query_scalar(
            "SELECT response FROM idempotency_keys WHERE operation = ? AND key = ?",
        )
        .bind(operation)
        .bind(key)
        .fetch_optional(&self.pool)
        .await?;

        response
            .map(|value| serde_json::from_str(&value))
            .transpose()
            .map_err(Into::into)
    }

    async fn store_idempotency_response(
        &self,
        operation: &str,
        key: &str,
        response: &Value,
    ) -> Result<()> {
        sqlx::query(
            r#"INSERT INTO idempotency_keys (operation, key, response)
               VALUES (?, ?, ?)
               ON CONFLICT(operation, key) DO NOTHING"#,
        )
        .bind(operation)
        .bind(key)
        .bind(serde_json::to_string(response)?)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn ensure_handle_available(&self, workspace_id: &str, handle: &str) -> Result<()> {
        let existing: Option<String> = sqlx::query_scalar(
            "SELECT session_id FROM sessions WHERE workspace_id = ? AND handle = ? AND state NOT IN ('exited', 'error') LIMIT 1",
        )
        .bind(workspace_id)
        .bind(handle)
        .fetch_optional(&self.pool)
        .await?;

        if existing.is_some() {
            return Err(Error::Conflict {
                code: "session_handle_conflict",
                message: format!(
                    "Cannot create session because {handle} is already used, please try a different handle."
                ),
            });
        }

        Ok(())
    }

    async fn upsert_runtime_binding(
        &self,
        session_id: &str,
        runtime: &RuntimeStartResult,
    ) -> Result<()> {
        sqlx::query(
            r#"INSERT INTO runtime_bindings (session_id, runtime_kind, runtime_ref, metadata)
               VALUES (?, ?, ?, ?)
               ON CONFLICT(session_id) DO UPDATE SET
                   runtime_kind = excluded.runtime_kind,
                   runtime_ref = excluded.runtime_ref,
                   metadata = excluded.metadata,
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')"#,
        )
        .bind(session_id)
        .bind(&runtime.runtime_kind)
        .bind(&runtime.runtime_ref)
        .bind(serde_json::to_string(&runtime.binding_metadata())?)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn update_session_workspace(
        &self,
        session_id: &str,
        workspace: Option<&WorkspaceRecord>,
    ) -> Result<()> {
        sqlx::query("UPDATE sessions SET workspace_ref = ?, workspace_id = ? WHERE session_id = ?")
            .bind(workspace.map(|workspace| workspace.canonical_path.as_str()))
            .bind(workspace.map(|workspace| workspace.workspace_id.as_str()))
            .bind(session_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
