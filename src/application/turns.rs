use super::*;

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct SubmitTurnRequest {
    pub input: String,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SubmitTurnOutcome {
    pub data: Value,
    pub duplicate: bool,
}

#[derive(Clone)]
pub struct TurnCommandService {
    pool: SqlitePool,
    runtime: GenericRuntimeManager,
}

impl TurnCommandService {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            runtime: GenericRuntimeManager,
        }
    }

    pub async fn submit_turn(
        &self,
        session_id: &str,
        request: SubmitTurnRequest,
        idempotency_key: Option<&str>,
    ) -> Result<SubmitTurnOutcome> {
        if let Some(key) = idempotency_key
            && let Some(response) = self
                .idempotency_response(&format!("submit_turn:{session_id}"), key)
                .await?
        {
            return Ok(SubmitTurnOutcome {
                data: response,
                duplicate: true,
            });
        }

        let query = ExternalQueryService::new(self.pool.clone());
        let session = query
            .get_session(session_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("session {session_id} not found")))?;

        if !matches!(session.state.as_str(), "idle" | "interrupted") {
            return Err(Error::StateConflict(format!(
                "session {session_id} in state {} cannot accept a new turn",
                session.state
            )));
        }

        if let Some(active_turn_id) = &session.current_turn_id {
            return Err(Error::StateConflict(format!(
                "session {session_id} already has active turn {active_turn_id}"
            )));
        }

        if !session.capabilities.accept_task {
            return Err(Error::Domain(format!(
                "session {session_id} runtime cannot accept tasks"
            )));
        }

        let turn_id = new_turn_id().to_string();
        let agent_input = AgentInput {
            session_id: session_id.to_string(),
            turn_id: turn_id.clone(),
            input: request.input.clone(),
        };
        if session.client_type == "generic" {
            self.runtime.submit_input(agent_input.clone())?;
        }

        let ingest = EventIngestService::new(self.pool.clone());
        ingest
            .ingest_event(DomainEvent::new(
                new_event_id().to_string(),
                session_id.to_string(),
                Some(turn_id.clone()),
                EventSource::ExternalApi,
                session.client_type.clone(),
                EventType::TurnCreated,
                json!({
                    "input": { "summary": request.input },
                    "metadata": request.metadata,
                }),
            ))
            .await?;
        ingest
            .ingest_event(DomainEvent::new(
                new_event_id().to_string(),
                session_id.to_string(),
                Some(turn_id.clone()),
                EventSource::ExternalApi,
                session.client_type.clone(),
                EventType::TurnQueued,
                json!({}),
            ))
            .await?;

        if session.client_type == "pi" {
            match self.runtime_binding_metadata(session_id).await? {
                Some((runtime_ref, metadata)) => {
                    match write_pi_current_turn_context(&metadata, &agent_input)
                        .and_then(|()| self.runtime.dispatch_pi_turn(&runtime_ref, &agent_input))
                    {
                        Ok(()) => {
                            ingest
                                .ingest_event(DomainEvent::new(
                                    new_event_id().to_string(),
                                    session_id.to_string(),
                                    Some(turn_id.clone()),
                                    EventSource::AgentAdapter,
                                    session.client_type.clone(),
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
                                    Some(turn_id.clone()),
                                    EventSource::RuntimeManager,
                                    session.client_type.clone(),
                                    EventType::TurnFailed,
                                    json!({ "failure": { "message": error.to_string() } }),
                                ))
                                .await?;
                        }
                    }
                }
                None => {
                    ingest
                        .ingest_event(DomainEvent::new(
                            new_event_id().to_string(),
                            session_id.to_string(),
                            Some(turn_id.clone()),
                            EventSource::RuntimeManager,
                            session.client_type.clone(),
                            EventType::TurnFailed,
                            json!({ "failure": { "message": "pi runtime binding not found" } }),
                        ))
                        .await?;
                }
            }
        }

        let mut turn = query
            .get_turn(session_id, &turn_id)
            .await?
            .ok_or_else(|| Error::Domain("submitted turn missing".to_string()))?;
        query.enrich_turn_view(&mut turn).await?;
        let data = json!({ "turn": turn });

        if let Some(key) = idempotency_key {
            self.store_idempotency_response(&format!("submit_turn:{session_id}"), key, &data)
                .await?;
        }

        Ok(SubmitTurnOutcome {
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

    async fn runtime_binding_metadata(&self, session_id: &str) -> Result<Option<(String, Value)>> {
        let row =
            sqlx::query("SELECT runtime_ref, metadata FROM runtime_bindings WHERE session_id = ?")
                .bind(session_id)
                .fetch_optional(&self.pool)
                .await?;
        row.map(|row| {
            let runtime_ref: String = row.try_get("runtime_ref")?;
            let metadata: String = row.try_get("metadata")?;
            let metadata = serde_json::from_str(&metadata)?;
            Ok((runtime_ref, metadata))
        })
        .transpose()
    }
}

fn write_pi_current_turn_context(metadata: &Value, input: &AgentInput) -> Result<()> {
    let current_turn_file = metadata["current_turn_file"]
        .as_str()
        .map(PathBuf::from)
        .or_else(|| {
            metadata["runtime_dir"]
                .as_str()
                .map(|runtime_dir| Path::new(runtime_dir).join("current-turn.json"))
        })
        .ok_or_else(|| {
            Error::Domain("pi runtime metadata missing current_turn_file".to_string())
        })?;
    if let Some(parent) = current_turn_file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let internal_event_url = metadata["internal_event_url"]
        .as_str()
        .unwrap_or("http://127.0.0.1:8080/internal/v1/events");
    let context = json!({
        "session_id": input.session_id,
        "turn_id": input.turn_id,
        "input": input.input,
        "client_type": "pi",
        "internal_event_url": internal_event_url,
    });
    std::fs::write(current_turn_file, serde_json::to_vec_pretty(&context)?)?;
    Ok(())
}
