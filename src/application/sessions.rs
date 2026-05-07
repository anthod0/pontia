use super::*;

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct CreateSessionRequest {
    #[serde(default = "default_client_type")]
    pub client_type: String,
    pub workspace: Option<String>,
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

        let workspace_record = if let Some(workspace) = request.workspace.as_deref() {
            Some(upsert_workspace(&self.pool, workspace).await?)
        } else {
            None
        };
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
