use super::*;
use crate::agent_clients::{ReadinessMode, get_client_spec};

fn client_readiness_mode(client_type: &str) -> Result<ReadinessMode> {
    get_client_spec(client_type)
        .map(|spec| spec.readiness_mode)
        .ok_or_else(|| Error::Domain(format!("unsupported client_type: {client_type}")))
}

#[derive(Debug, Clone, PartialEq)]
pub struct ControlCommandOutcome {
    pub data: Value,
    pub duplicate: bool,
}

#[derive(Clone)]
pub struct RuntimeControlService {
    pool: SqlitePool,
    runtime: GenericRuntimeManager,
}

impl RuntimeControlService {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            runtime: GenericRuntimeManager,
        }
    }

    pub async fn interrupt_current_turn(
        &self,
        session_id: &str,
        idempotency_key: Option<&str>,
    ) -> Result<ControlCommandOutcome> {
        if let Some(key) = idempotency_key
            && let Some(response) = self
                .idempotency_response(&format!("interrupt_current:{session_id}"), key)
                .await?
        {
            return Ok(ControlCommandOutcome {
                data: response,
                duplicate: true,
            });
        }

        let query = ExternalQueryService::new(self.pool.clone());
        let session = query
            .get_session(session_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("session {session_id} not found")))?;
        let turn_id = session.current_turn_id.clone().ok_or_else(|| {
            Error::StateConflict(format!(
                "session {session_id} has no active turn to interrupt"
            ))
        })?;
        let outcome = self.interrupt_turn(session_id, &turn_id, None).await?;
        if let Some(key) = idempotency_key {
            self.store_idempotency_response(
                &format!("interrupt_current:{session_id}"),
                key,
                &outcome.data,
            )
            .await?;
        }
        Ok(outcome)
    }

    pub async fn interrupt_turn(
        &self,
        session_id: &str,
        turn_id: &str,
        idempotency_key: Option<&str>,
    ) -> Result<ControlCommandOutcome> {
        if let Some(key) = idempotency_key
            && let Some(response) = self
                .idempotency_response(&format!("interrupt_turn:{session_id}:{turn_id}"), key)
                .await?
        {
            return Ok(ControlCommandOutcome {
                data: response,
                duplicate: true,
            });
        }

        let query = ExternalQueryService::new(self.pool.clone());
        let session = query
            .get_session(session_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("session {session_id} not found")))?;
        let turn = query
            .get_turn(session_id, turn_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("turn {turn_id} not found")))?;

        if matches!(
            turn.state.as_str(),
            "completed" | "failed" | "interrupted" | "cancelled"
        ) {
            return Err(Error::StateConflict(format!(
                "turn {turn_id} is already terminal"
            )));
        }
        if session.current_turn_id.as_deref() != Some(turn_id) {
            return Err(Error::StateConflict(format!(
                "turn {turn_id} is not the active turn for session {session_id}"
            )));
        }
        if !session.capabilities.interrupt {
            return Err(Error::CapabilityUnavailable(format!(
                "session {session_id} runtime does not support interrupt"
            )));
        }
        let runtime_ref = self.runtime_ref(session_id).await?.ok_or_else(|| {
            Error::StateConflict(format!("session {session_id} has no runtime binding"))
        })?;
        self.runtime.interrupt_session(&runtime_ref)?;

        let ingest = EventIngestService::new(self.pool.clone());
        ingest
            .ingest_event(DomainEvent::new(
                new_event_id().to_string(),
                session_id.to_string(),
                Some(turn_id.to_string()),
                EventSource::ExternalApi,
                session.client_type.clone(),
                EventType::TurnInterruptRequested,
                json!({}),
            ))
            .await?;
        ingest
            .ingest_event(DomainEvent::new(
                new_event_id().to_string(),
                session_id.to_string(),
                Some(turn_id.to_string()),
                EventSource::RuntimeManager,
                session.client_type,
                EventType::TurnInterrupted,
                json!({}),
            ))
            .await?;

        let turn = query
            .get_turn(session_id, turn_id)
            .await?
            .ok_or_else(|| Error::Domain("interrupted turn missing".to_string()))?;
        let data = json!({ "turn": turn });
        if let Some(key) = idempotency_key {
            self.store_idempotency_response(
                &format!("interrupt_turn:{session_id}:{turn_id}"),
                key,
                &data,
            )
            .await?;
        }
        Ok(ControlCommandOutcome {
            data,
            duplicate: false,
        })
    }

    pub async fn terminate_session(
        &self,
        session_id: &str,
        idempotency_key: Option<&str>,
    ) -> Result<ControlCommandOutcome> {
        if let Some(key) = idempotency_key
            && let Some(response) = self
                .idempotency_response(&format!("terminate_session:{session_id}"), key)
                .await?
        {
            return Ok(ControlCommandOutcome {
                data: response,
                duplicate: true,
            });
        }

        let query = ExternalQueryService::new(self.pool.clone());
        let session = query
            .get_session(session_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("session {session_id} not found")))?;

        if !matches!(session.state.as_str(), "exited" | "error") {
            if let Some(runtime_ref) = self.runtime_ref(session_id).await? {
                self.runtime.terminate_session(&runtime_ref)?;
            }
            EventIngestService::new(self.pool.clone())
                .ingest_event(DomainEvent::new(
                    new_event_id().to_string(),
                    session_id.to_string(),
                    None,
                    EventSource::RuntimeManager,
                    session.client_type,
                    EventType::SessionExited,
                    json!({}),
                ))
                .await?;
        }

        let session = query
            .get_session(session_id)
            .await?
            .ok_or_else(|| Error::Domain("terminated session missing".to_string()))?;
        let data = json!({ "session": session });
        if let Some(key) = idempotency_key {
            self.store_idempotency_response(&format!("terminate_session:{session_id}"), key, &data)
                .await?;
        }
        Ok(ControlCommandOutcome {
            data,
            duplicate: false,
        })
    }

    pub async fn restart_session(
        &self,
        session_id: &str,
        idempotency_key: Option<&str>,
    ) -> Result<ControlCommandOutcome> {
        if let Some(key) = idempotency_key
            && let Some(response) = self
                .idempotency_response(&format!("restart_session:{session_id}"), key)
                .await?
        {
            return Ok(ControlCommandOutcome {
                data: response,
                duplicate: true,
            });
        }

        let query = ExternalQueryService::new(self.pool.clone());
        let session = query
            .get_session(session_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("session {session_id} not found")))?;
        if matches!(session.state.as_str(), "exited" | "error") {
            return Err(Error::StateConflict(format!(
                "terminal session {session_id} cannot be restarted"
            )));
        }

        let prior_restart_count = self.restart_count(session_id).await?.unwrap_or(0);
        if let Some(runtime_ref) = self.runtime_ref(session_id).await? {
            self.runtime.terminate_session(&runtime_ref)?;
        }

        let ingest = EventIngestService::new(self.pool.clone());
        ingest
            .ingest_event(DomainEvent::new(
                new_event_id().to_string(),
                session_id.to_string(),
                None,
                EventSource::ExternalApi,
                session.client_type.clone(),
                EventType::SessionStarting,
                json!({}),
            ))
            .await?;
        let runtime = self.runtime.start_session_with_restart_count(
            RuntimeStartRequest {
                session_id: session_id.to_string(),
                client_type: session.client_type.clone(),
                workspace: session.workspace.clone(),
                handle: session.handle.clone(),
                role: session.role.clone(),
            },
            prior_restart_count + 1,
        )?;
        self.upsert_runtime_binding(session_id, &runtime).await?;
        ingest
            .ingest_event(DomainEvent::new(
                new_event_id().to_string(),
                session_id.to_string(),
                None,
                EventSource::RuntimeManager,
                session.client_type.clone(),
                EventType::SessionStarted,
                json!({}),
            ))
            .await?;
        if client_readiness_mode(&session.client_type)? == ReadinessMode::RuntimeManagerImmediate {
            ingest
                .ingest_event(DomainEvent::new(
                    new_event_id().to_string(),
                    session_id.to_string(),
                    None,
                    EventSource::RuntimeManager,
                    session.client_type,
                    EventType::SessionReady,
                    json!({}),
                ))
                .await?;
        }

        let session = query
            .get_session(session_id)
            .await?
            .ok_or_else(|| Error::Domain("restarted session missing".to_string()))?;
        let data = json!({ "session": session });
        if let Some(key) = idempotency_key {
            self.store_idempotency_response(&format!("restart_session:{session_id}"), key, &data)
                .await?;
        }
        Ok(ControlCommandOutcome {
            data,
            duplicate: false,
        })
    }

    async fn runtime_ref(&self, session_id: &str) -> Result<Option<String>> {
        sqlx::query_scalar("SELECT runtime_ref FROM runtime_bindings WHERE session_id = ?")
            .bind(session_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(Into::into)
    }

    async fn restart_count(&self, session_id: &str) -> Result<Option<i64>> {
        let metadata: Option<String> =
            sqlx::query_scalar("SELECT metadata FROM runtime_bindings WHERE session_id = ?")
                .bind(session_id)
                .fetch_optional(&self.pool)
                .await?;
        metadata
            .map(|metadata| {
                serde_json::from_str::<Value>(&metadata)
                    .map(|value| value["restart_count"].as_i64().unwrap_or(0))
            })
            .transpose()
            .map_err(Into::into)
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
}
