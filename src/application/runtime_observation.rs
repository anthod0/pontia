use super::*;
use crate::agent_clients::{self, AdapterEventBehavior};

#[derive(Clone)]
pub struct RuntimeObservationService {
    pool: SqlitePool,
    runtime: GenericRuntimeManager,
}

impl RuntimeObservationService {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            runtime: GenericRuntimeManager,
        }
    }

    pub async fn observe_session(&self, session_id: &str) -> Result<()> {
        let query = ExternalQueryService::new(self.pool.clone());
        let session = query
            .get_session(session_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("session {session_id} not found")))?;
        if matches!(session.state.as_str(), "exited" | "error") {
            return Ok(());
        }

        let runtime_ref: Option<String> =
            sqlx::query_scalar("SELECT runtime_ref FROM runtime_bindings WHERE session_id = ?")
                .bind(session_id)
                .fetch_optional(&self.pool)
                .await?;
        let Some(runtime_ref) = runtime_ref else {
            return Ok(());
        };
        if self.runtime.is_alive(&runtime_ref) {
            return Ok(());
        }

        let ingest = EventIngestService::new(self.pool.clone());
        if let Some(turn_id) = session.current_turn_id.clone() {
            ingest
                .ingest_event(DomainEvent::new(
                    new_event_id().to_string(),
                    session_id.to_string(),
                    Some(turn_id),
                    EventSource::RuntimeManager,
                    session.client_type.clone(),
                    EventType::TurnFailed,
                    json!({ "failure": { "message": "runtime tmux session is not alive" } }),
                ))
                .await?;
        }
        ingest
            .ingest_event(DomainEvent::new(
                new_event_id().to_string(),
                session_id.to_string(),
                None,
                EventSource::RuntimeManager,
                session.client_type,
                EventType::SessionError,
                json!({ "failure": { "message": "runtime tmux session is not alive" } }),
            ))
            .await?;
        Ok(())
    }
}

#[derive(Clone)]
pub struct AdapterEventOutboxService {
    pool: SqlitePool,
}

impl AdapterEventOutboxService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn observe_session(&self, session_id: &str) -> Result<()> {
        let query = ExternalQueryService::new(self.pool.clone());
        let session = query
            .get_session(session_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("session {session_id} not found")))?;
        let Some(client_spec) = agent_clients::get_client_spec(&session.client_type) else {
            return Ok(());
        };
        if !matches!(
            client_spec.adapter_events,
            AdapterEventBehavior::JsonlOutbox { .. }
        ) {
            return Ok(());
        }

        let Some(adapter_event_log) = self.adapter_event_log(session_id).await? else {
            return Ok(());
        };
        if !Path::new(&adapter_event_log).exists() {
            return Ok(());
        }

        let content = std::fs::read_to_string(&adapter_event_log)?;
        let ingest = EventIngestService::new(self.pool.clone());
        for (line_index, line) in content.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            match adapter_record_to_event(line, session_id, &session.client_type) {
                Ok(event) => {
                    ingest.ingest_event(event).await?;
                }
                Err(error) => {
                    ingest
                        .ingest_event(DomainEvent::new(
                            new_event_id().to_string(),
                            session_id.to_string(),
                            None,
                            EventSource::AgentAdapter,
                            session.client_type.clone(),
                            EventType::SessionError,
                            json!({
                                "adapter_error": {
                                    "kind": "malformed_record",
                                    "line": line_index + 1,
                                    "message": error.to_string(),
                                }
                            }),
                        ))
                        .await?;
                }
            }
        }
        Ok(())
    }

    async fn adapter_event_log(&self, session_id: &str) -> Result<Option<String>> {
        let metadata: Option<String> =
            sqlx::query_scalar("SELECT metadata FROM runtime_bindings WHERE session_id = ?")
                .bind(session_id)
                .fetch_optional(&self.pool)
                .await?;
        metadata
            .map(|metadata| {
                serde_json::from_str::<Value>(&metadata)
                    .map(|value| value["adapter_event_log"].as_str().map(ToString::to_string))
            })
            .transpose()
            .map_err(Into::into)
            .map(Option::flatten)
    }
}

fn adapter_record_to_event(line: &str, session_id: &str, client_type: &str) -> Result<DomainEvent> {
    let value: Value = serde_json::from_str(line)?;
    let record_session_id = value["session_id"]
        .as_str()
        .ok_or_else(|| Error::Domain("adapter event missing session_id".to_string()))?;
    if record_session_id != session_id {
        return Err(Error::Domain(format!(
            "adapter event session_id {record_session_id} does not match {session_id}"
        )));
    }

    let turn_id = value["turn_id"]
        .as_str()
        .ok_or_else(|| Error::Domain("adapter event missing turn_id".to_string()))?;
    let event_type = value["type"]
        .as_str()
        .ok_or_else(|| Error::Domain("adapter event missing type".to_string()))?;
    let event_type = EventType::from_str(event_type)?;
    if !matches!(
        event_type,
        EventType::TurnOutput | EventType::TurnCompleted | EventType::TurnFailed
    ) {
        return Err(Error::Domain(format!(
            "adapter event type {event_type} is not accepted from adapter outbox"
        )));
    }
    let payload = value.get("payload").cloned().unwrap_or_else(|| json!({}));
    if !payload.is_object() {
        return Err(Error::Domain(
            "adapter event payload must be a JSON object".to_string(),
        ));
    }

    Ok(DomainEvent::new(
        new_event_id().to_string(),
        session_id.to_string(),
        Some(turn_id.to_string()),
        EventSource::AgentAdapter,
        client_type.to_string(),
        event_type,
        payload,
    ))
}
