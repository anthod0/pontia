mod effects;
mod enrichment;
mod validation;

use sqlx::SqlitePool;

use pontia_core::{
    domain::{
        DomainEvent, EventType, ProjectionState, ReportedEvent, SessionProjection, TurnProjection,
        TurnTopology,
    },
    error::{Error, Result},
};
use pontia_storage_sqlite::repositories::{
    events::{EventInsertRecord, SqliteEventRepository},
    sessions::{SessionProjectionUpsertRecord, SqliteSessionRepository},
    turns::{SqliteTurnRepository, TurnProjectionUpsertRecord},
};

use self::enrichment::{consume_transient_pi_native_evidence, should_resolve_pi_topology};
use super::{EventIngestResult, PontiaEvent};
use crate::{
    AgentEventBroker, InboxCommandService, UpsertAgentBindingRequest, row_to_event, row_to_session,
    row_to_turn,
};

#[derive(Clone)]
pub struct EventIngestService {
    pool: SqlitePool,
    agent_events: Option<AgentEventBroker>,
}

impl EventIngestService {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            agent_events: None,
        }
    }

    pub fn with_agent_events(mut self, agent_events: AgentEventBroker) -> Self {
        self.agent_events = Some(agent_events);
        self
    }

    pub async fn ingest_pontia_event(&self, event: PontiaEvent) -> Result<EventIngestResult> {
        self.ingest_domain_event(event.into_reported_event().into(), None, false)
            .await
    }

    /// Ingests a Pontia runtime observation while fencing it to the currently
    /// bound runtime instance.
    pub async fn ingest_runtime_observation_event(
        &self,
        event: PontiaEvent,
    ) -> Result<EventIngestResult> {
        self.ingest_domain_event(event.into_reported_event().into(), None, true)
            .await
    }

    /// Ingests a fact supplied by an explicit agent-client adapter.
    ///
    /// This path preserves adapter and replay behavior that predates runtime
    /// fencing. HTTP reports must use [`Self::ingest_confirmed_event`], while
    /// Pontia-owned callers must use [`Self::ingest_pontia_event`].
    pub async fn ingest_reported_event(&self, event: ReportedEvent) -> Result<EventIngestResult> {
        self.ingest_domain_event(event.into(), None, false).await
    }

    pub(crate) async fn ingest_in_process_ready_event(
        &self,
        client_type: &str,
        session_id: &str,
        runtime_instance_id: Option<&str>,
    ) -> Result<()> {
        let Some(event) = runtime_instance_id.and_then(|runtime_instance_id| {
            pontia_agent_clients::in_process_ready_event(
                client_type,
                session_id,
                runtime_instance_id,
            )
        }) else {
            return Ok(());
        };
        self.ingest_reported_event(event).await?;
        Ok(())
    }

    pub async fn ingest_confirmed_event(&self, event: ReportedEvent) -> Result<EventIngestResult> {
        self.ingest_domain_event(event.into(), None, true).await
    }

    pub async fn ingest_event_with_topology(
        &self,
        event: ReportedEvent,
        topology: TurnTopology,
    ) -> Result<EventIngestResult> {
        let mut event: DomainEvent = event.into();
        event.topology = Some(topology);
        self.ingest_domain_event(event, None, false).await
    }

    pub(crate) async fn ingest_pontia_event_with_agent_binding(
        &self,
        event: PontiaEvent,
        binding: UpsertAgentBindingRequest,
    ) -> Result<EventIngestResult> {
        self.ingest_domain_event(event.into_reported_event().into(), Some(binding), false)
            .await
    }

    async fn ingest_domain_event(
        &self,
        mut event: DomainEvent,
        initial_agent_binding: Option<UpsertAgentBindingRequest>,
        enforce_runtime_fence: bool,
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
            self.clear_exited_session_tmux_markers(&event, false).await;
            return Ok(EventIngestResult {
                accepted: true,
                duplicate: true,
                event_id: event.event_id,
                session_id: event.session_id,
                turn_id: event.turn_id,
                state_version: existing_version,
            });
        }

        self.enrich_timeline_boundary(&mut event).await;
        let topology_evidence = consume_transient_pi_native_evidence(&mut event);
        let topology_binding_id = if should_resolve_pi_topology(&event) {
            crate::AgentBindingService::new(self.pool.clone())
                .binding_for_session(&event.session_id)
                .await
                .ok()
                .flatten()
                .map(|binding| binding.id)
        } else {
            None
        };

        let mut tx = self.pool.begin().await?;
        if event.event_type != EventType::SessionCreated {
            let session_exists =
                SqliteTurnRepository::serialize_session_turn_writes_if_exists_in_tx(
                    &mut tx,
                    &event.session_id,
                )
                .await?;
            if !session_exists && (event.event_type.is_turn_event() || enforce_runtime_fence) {
                SqliteTurnRepository::serialize_session_turn_writes_in_tx(
                    &mut tx,
                    &event.session_id,
                )
                .await?;
            }
            if enforce_runtime_fence {
                self.ensure_runtime_fence_in_tx(&mut tx, &event).await?;
            }
        }
        self.validate_turn_identity_in_tx(&mut tx, &event, enforce_runtime_fence)
            .await?;
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
        self.enrich_pi_topology(&mut event, topology_binding_id, topology_evidence, &turns);
        let mut projection = ProjectionState::with_existing(sessions, turns);
        projection.apply(&event)?;

        let payload = serde_json::to_string(&event.payload)?;
        let timeline_boundary = event
            .timeline_boundary
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;
        let turn_topology = event
            .topology
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;
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
                payload,
                timeline_boundary,
                turn_topology,
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
                        head_cursor: turn.head_cursor.clone(),
                        tail_cursor: turn.tail_cursor.clone(),
                        parent_turn_id: turn.topology.parent_turn_id().map(ToString::to_string),
                        topology_status: turn.topology.status().to_string(),
                        state: turn.state.to_string(),
                        state_version: turn.state_version,
                        input_summary: turn.input_summary.clone(),
                        output_summary: turn.output_summary.clone(),
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

        if let Some(agent_events) = &self.agent_events {
            agent_events.publish(event.clone());
        }

        self.clear_exited_session_tmux_markers(&event, true).await;
        self.link_started_turn_to_inbox_message(&event).await?;

        if matches!(
            event.event_type,
            EventType::SessionReady
                | EventType::TurnCompleted
                | EventType::TurnFailed
                | EventType::TurnDispatchFailed
                | EventType::TurnAbandoned
                | EventType::TurnInterrupted
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

    pub async fn volatile_state_version(&self, session_id: &str) -> Result<i64> {
        SqliteEventRepository::new(self.pool.clone())
            .session_event_count(session_id)
            .await
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
