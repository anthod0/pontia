use serde_json::Value;
use sqlx::SqlitePool;

use pontia_core::{
    domain::{
        DomainEvent, EventSource, EventType, ProjectionState, ReportedEvent, SessionProjection,
        TurnProjection,
    },
    error::{Error, Result},
};
use pontia_storage_sqlite::repositories::{
    agent_bindings::SqliteAgentBindingRepository,
    events::{EventInsertRecord, SqliteEventRepository},
    inbox::SqliteInboxRepository,
    runtime_bindings::SqliteRuntimeBindingRepository,
    sessions::{SessionProjectionUpsertRecord, SqliteSessionRepository},
    turns::{SqliteTurnRepository, TurnProjectionUpsertRecord},
};

use super::EventIngestResult;
use crate::{
    InboxCommandService, UpsertAgentBindingRequest, row_to_event, row_to_session, row_to_turn,
};

#[derive(Clone)]
pub struct EventIngestService {
    pool: SqlitePool,
}

impl EventIngestService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn ingest_event(&self, event: ReportedEvent) -> Result<EventIngestResult> {
        self.ingest_domain_event(event.into(), None).await
    }

    pub(crate) async fn ingest_event_with_agent_binding(
        &self,
        event: ReportedEvent,
        binding: UpsertAgentBindingRequest,
    ) -> Result<EventIngestResult> {
        self.ingest_domain_event(event.into(), Some(binding)).await
    }

    async fn ingest_domain_event(
        &self,
        mut event: DomainEvent,
        initial_agent_binding: Option<UpsertAgentBindingRequest>,
    ) -> Result<EventIngestResult> {
        if event.event_type.is_turn_event() && event.turn_id.is_none() {
            return Err(Error::Domain(format!(
                "{} must carry turn_id",
                event.event_type
            )));
        }
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

        let mut tx = self.pool.begin().await?;
        self.enrich_turn_index_in_tx(&mut tx, &mut event).await?;
        let sessions =
            SqliteSessionRepository::load_projection_rows_in_tx(&mut tx, &event.session_id)
                .await?
                .into_iter()
                .map(row_to_session)
                .collect::<Result<Vec<_>>>()?;
        let turns = SqliteTurnRepository::load_projection_rows_in_tx(&mut tx, &event.session_id)
            .await?
            .into_iter()
            .map(row_to_turn)
            .collect::<Result<Vec<_>>>()?;
        let mut projection = ProjectionState::with_existing(sessions, turns);
        projection.apply(&event)?;

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
                turn_index: event.turn_index,
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
                        turn_index: turn.turn_index,
                        state: turn.state.to_string(),
                        state_version: turn.state_version,
                        metadata,
                    },
                )
                .await?;
            }
        }

        if let Some(binding) = initial_agent_binding {
            crate::agent_bindings::upsert_agent_binding_in_tx(&mut tx, binding).await?;
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

    async fn enrich_turn_index_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        event: &mut DomainEvent,
    ) -> Result<()> {
        if !event.event_type.is_turn_event() {
            if event.turn_index.is_some() {
                return Err(Error::Domain(
                    "session event cannot carry Pontia-owned turn_index".to_string(),
                ));
            }
            return Ok(());
        }
        let turn_id = event.turn_id.as_deref().expect("validated turn_id");

        SqliteTurnRepository::serialize_session_turn_writes_in_tx(tx, &event.session_id).await?;
        let persisted =
            SqliteTurnRepository::turn_index_in_tx(tx, &event.session_id, turn_id).await?;
        let turn_index = match persisted {
            Some(turn_index) => turn_index,
            None => SqliteTurnRepository::allocate_turn_index_in_tx(tx, &event.session_id).await?,
        };
        event.turn_index = Some(turn_index);
        Ok(())
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

        let session = SqliteSessionRepository::new(self.pool.clone())
            .get_session(&event.session_id)
            .await?;
        let Some(session) = session else {
            return Err(Error::Domain(format!(
                "{} from {} references unknown session {}",
                event.event_type, event.source, event.session_id
            )));
        };
        if event.client_type != session.client_type {
            return Err(Error::Domain(format!(
                "{} from {} has client_type {} but session {} uses client_type {}",
                event.event_type,
                event.source,
                event.client_type,
                event.session_id,
                session.client_type
            )));
        }

        if event.event_type == EventType::SessionReady {
            self.ensure_ready_identity_matches_bindings(event).await?;
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

    async fn ensure_ready_identity_matches_bindings(&self, event: &DomainEvent) -> Result<()> {
        let Some(client_session_key) = event
            .payload
            .get("client_session_key")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return Ok(());
        };

        let agent_bindings = SqliteAgentBindingRepository::new(self.pool.clone());
        if let Some(binding) = agent_bindings
            .binding_for_session(&event.session_id)
            .await?
            && (binding.client_type != event.client_type
                || binding.client_session_key != client_session_key)
        {
            return Err(Error::Domain(format!(
                "session.ready client identity does not match session {} Agent binding",
                event.session_id
            )));
        }
        if let Some(binding) = agent_bindings
            .binding_for_client_session(&event.client_type, client_session_key)
            .await?
            && binding.session_id != event.session_id
        {
            return Err(Error::Domain(format!(
                "session.ready client identity is already bound to another Session {}",
                binding.session_id
            )));
        }

        if let Some(runtime_metadata) = SqliteRuntimeBindingRepository::new(self.pool.clone())
            .metadata(&event.session_id)
            .await?
            && crate::agent_bindings::runtime_binding_identity_disagrees(
                &runtime_metadata,
                client_session_key,
            )?
        {
            return Err(Error::Domain(format!(
                "session.ready client identity does not match session {} Runtime binding",
                event.session_id
            )));
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
