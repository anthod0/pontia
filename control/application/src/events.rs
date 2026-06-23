use super::*;
use pontia_storage_sqlite::repositories::{
    events::{EventInsertRecord, SqliteEventRepository},
    inbox::SqliteInboxRepository,
    runtime_bindings::SqliteRuntimeBindingRepository,
    sessions::{SessionProjectionUpsertRecord, SqliteSessionRepository},
    turns::{SqliteTurnRepository, TurnProjectionUpsertRecord},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventIngestResult {
    pub accepted: bool,
    pub duplicate: bool,
    pub event_id: String,
    pub session_id: String,
    pub turn_id: Option<String>,
    pub state_version: i64,
}

#[derive(Clone, Default)]
pub struct InternalEventValidationService;

impl InternalEventValidationService {
    pub fn new() -> Self {
        Self
    }

    pub fn validate(&self, event: &DomainEvent) -> Result<()> {
        if event.event_type == EventType::SessionReady && event.source == EventSource::AgentClient {
            let runtime_instance_id = event
                .payload
                .get("runtime_instance_id")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if runtime_instance_id.trim().is_empty() {
                return Err(Error::Domain(
                    "session.ready from agent_client requires payload.runtime_instance_id"
                        .to_string(),
                ));
            }
            if pontia_agent_clients::client_session_identity_required_on_ready(&event.client_type) {
                let client_session_key = event
                    .payload
                    .get("client_session_key")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if client_session_key.trim().is_empty() {
                    return Err(Error::Domain(format!(
                        "{} session.ready from agent_client requires payload.client_session_key",
                        event.client_type
                    )));
                }
            }
        }

        Ok(())
    }
}

#[derive(Clone)]
pub struct EventIngestService {
    pool: SqlitePool,
}

impl EventIngestService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn ingest_event(&self, event: DomainEvent) -> Result<EventIngestResult> {
        if let Some(existing_version) = self
            .existing_event_state_version(&event.event_id, &event.session_id)
            .await?
        {
            return Ok(EventIngestResult {
                accepted: true,
                duplicate: true,
                event_id: event.event_id,
                session_id: event.session_id,
                turn_id: event.turn_id,
                state_version: existing_version,
            });
        }

        let sessions = self.load_session_projection(&event.session_id).await?;
        let turns = self.load_turn_projections(&event.session_id).await?;
        let mut projection = ProjectionState::with_existing(sessions, turns);
        projection.apply(&event)?;

        let mut tx = self.pool.begin().await?;
        let payload = serde_json::to_string(&event.payload)?;
        let occurred_at = event
            .occurred_at
            .format(&time::format_description::well_known::Rfc3339)
            .map_err(|err| {
                pontia_core::error::Error::Domain(format!("invalid event timestamp: {err}"))
            })?;

        SqliteEventRepository::insert_event_in_tx(
            &mut tx,
            EventInsertRecord {
                event_id: event.event_id.clone(),
                session_id: event.session_id.clone(),
                turn_id: event.turn_id.clone(),
                source: event.source.to_string(),
                client_type: event.client_type.clone(),
                event_type: event.event_type.to_string(),
                occurred_at,
                seq: event.seq,
                payload,
            },
        )
        .await?;

        let state_version =
            SqliteEventRepository::session_event_count_in_tx(&mut tx, &event.session_id).await?;

        if event.event_type != EventType::SessionMessageUpdated {
            for session in projection.sessions() {
                let metadata = serde_json::to_string(&session.metadata)?;
                SqliteSessionRepository::upsert_projection_in_tx(
                    &mut tx,
                    SessionProjectionUpsertRecord {
                        session_id: session.session_id.clone(),
                        client_type: session.client_type.clone(),
                        title: session.title.clone(),
                        handle: session.handle.clone(),
                        role: session.role.clone(),
                        description: session.description.clone(),
                        execution_profile_id: session.execution_profile_id.clone(),
                        execution_profile_version: session.execution_profile_version.clone(),
                        state: session.state.to_string(),
                        current_turn_id: session.current_turn_id.clone(),
                        state_version,
                        metadata,
                    },
                )
                .await?;
            }

            for turn in projection.turns() {
                let metadata = serde_json::to_string(&turn.metadata)?;
                SqliteTurnRepository::upsert_projection_in_tx(
                    &mut tx,
                    TurnProjectionUpsertRecord {
                        turn_id: turn.turn_id.clone(),
                        session_id: turn.session_id.clone(),
                        state: turn.state.to_string(),
                        state_version: turn.state_version,
                        metadata,
                    },
                )
                .await?;
            }
        }

        crate::agent_bindings::register_agent_binding_for_ready_event_in_tx(&mut tx, &event)
            .await?;

        tx.commit().await?;

        self.link_started_turn_to_inbox_message(&event).await?;

        if matches!(
            event.event_type,
            EventType::SessionReady
                | EventType::TurnCompleted
                | EventType::TurnFailed
                | EventType::TurnInterrupted
                | EventType::TurnCancelled
        ) {
            Box::pin(InboxCommandService::new(self.pool.clone()).drain_inbox(&event.session_id))
                .await?;
        }

        Ok(EventIngestResult {
            accepted: true,
            duplicate: false,
            event_id: event.event_id,
            session_id: event.session_id,
            turn_id: event.turn_id,
            state_version,
        })
    }

    async fn link_started_turn_to_inbox_message(&self, event: &DomainEvent) -> Result<()> {
        if event.event_type != EventType::TurnStarted {
            return Ok(());
        }
        let Some(turn_id) = event.turn_id.as_deref() else {
            return Ok(());
        };
        let inbox_message_id = event
            .payload
            .pointer("/metadata/inbox_message_id")
            .or_else(|| event.payload.pointer("/input/inbox_message_id"))
            .and_then(Value::as_str);
        let Some(inbox_message_id) = inbox_message_id else {
            return Ok(());
        };

        SqliteInboxRepository::new(self.pool.clone())
            .link_started_turn(&event.session_id, inbox_message_id, turn_id)
            .await
    }

    pub async fn get_session(&self, session_id: &str) -> Result<Option<SessionProjection>> {
        let mut sessions = self.load_session_projection(session_id).await?;
        Ok(sessions.pop())
    }

    pub async fn get_turn(&self, turn_id: &str) -> Result<Option<TurnProjection>> {
        SqliteTurnRepository::new(self.pool.clone())
            .get_projection(turn_id)
            .await?
            .map(row_to_turn)
            .transpose()
    }

    pub async fn list_events(&self, session_id: &str) -> Result<Vec<DomainEvent>> {
        let rows = SqliteEventRepository::new(self.pool.clone())
            .list_domain_event_rows(session_id)
            .await?;

        rows.into_iter().map(row_to_event).collect()
    }

    pub async fn sequence_warnings(&self, event: &DomainEvent) -> Result<Vec<String>> {
        let Some(seq) = event.seq else {
            return Ok(Vec::new());
        };

        let max_seq = SqliteEventRepository::new(self.pool.clone())
            .max_seq(&event.session_id)
            .await?;

        let Some(max_seq) = max_seq else {
            return Ok(Vec::new());
        };

        let warning = if seq <= max_seq {
            Some(format!(
                "non-monotonic sequence: received seq {seq} after max seq {max_seq}"
            ))
        } else if seq > max_seq + 1 {
            Some(format!(
                "sequence gap: received seq {seq} after max seq {max_seq}"
            ))
        } else {
            None
        };

        Ok(warning.into_iter().collect())
    }

    pub async fn record_warnings(&self, event: &DomainEvent, warnings: &[String]) -> Result<()> {
        SqliteEventRepository::new(self.pool.clone())
            .record_warnings(&event.event_id, &event.session_id, warnings)
            .await
    }

    pub async fn volatile_state_version(&self, session_id: &str) -> Result<i64> {
        SqliteEventRepository::new(self.pool.clone())
            .session_event_count(session_id)
            .await
    }

    pub async fn ensure_confirmed_event_matches_session_boundary(
        &self,
        event: &DomainEvent,
    ) -> Result<()> {
        if !is_confirmed_runtime_source(event.source)
            || event.event_type == EventType::SessionCreated
        {
            return Ok(());
        }

        let session_exists = SqliteSessionRepository::new(self.pool.clone())
            .exists(&event.session_id)
            .await?;
        if !session_exists {
            return Err(Error::Domain(format!(
                "{} from {} references unknown session {}",
                event.event_type, event.source, event.session_id
            )));
        }

        let expected_runtime_instance_id = SqliteRuntimeBindingRepository::new(self.pool.clone())
            .runtime_instance_id(&event.session_id)
            .await?;

        let Some(expected_runtime_instance_id) = expected_runtime_instance_id else {
            return Ok(());
        };

        let provided_runtime_instance_id = event
            .payload
            .get("runtime_instance_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty());

        if runtime_instance_id_required_for_event(event.event_type)
            || event.payload.get("runtime_instance_id").is_some()
        {
            let Some(provided_runtime_instance_id) = provided_runtime_instance_id else {
                return Err(Error::Domain(format!(
                    "{} from {} requires payload.runtime_instance_id for runtime-bound session {}",
                    event.event_type, event.source, event.session_id
                )));
            };
            if provided_runtime_instance_id != expected_runtime_instance_id {
                return Err(Error::Domain(format!(
                    "payload.runtime_instance_id does not match session {} runtime binding",
                    event.session_id
                )));
            }
        }

        Ok(())
    }

    async fn existing_event_state_version(
        &self,
        event_id: &str,
        session_id: &str,
    ) -> Result<Option<i64>> {
        SqliteEventRepository::new(self.pool.clone())
            .existing_event_state_version(event_id, session_id)
            .await
    }

    async fn load_session_projection(&self, session_id: &str) -> Result<Vec<SessionProjection>> {
        let rows = SqliteSessionRepository::new(self.pool.clone())
            .load_projection_rows(session_id)
            .await?;

        rows.into_iter().map(row_to_session).collect()
    }

    async fn load_turn_projections(&self, session_id: &str) -> Result<Vec<TurnProjection>> {
        let rows = SqliteTurnRepository::new(self.pool.clone())
            .load_projection_rows(session_id)
            .await?;

        rows.into_iter().map(row_to_turn).collect()
    }
}

fn is_confirmed_runtime_source(source: EventSource) -> bool {
    matches!(source, EventSource::AgentAdapter | EventSource::AgentClient)
}

fn runtime_instance_id_required_for_event(event_type: EventType) -> bool {
    matches!(
        event_type,
        EventType::SessionReady | EventType::SessionExited | EventType::TurnStarted
    )
}

pub(crate) fn nested_string(value: &Value, path: &[&str]) -> Option<String> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    current.as_str().map(ToString::to_string)
}

pub(crate) fn nested_array_strings(value: &Value, path: &[&str]) -> Option<Vec<String>> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    Some(
        current
            .as_array()?
            .iter()
            .filter_map(Value::as_str)
            .map(ToString::to_string)
            .collect(),
    )
}

pub(crate) fn remove_internal_metadata_fields(value: &mut Value) {
    if let Some(object) = value.as_object_mut() {
        object.remove("source_ref");
    }
}
