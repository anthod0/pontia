mod effects;
mod enrichment;
mod persistence;
mod validation;

use sqlx::SqlitePool;

use pontia_core::{
    domain::{
        DomainEvent, EventType, ProjectionState, ReportedEvent, SessionProjection, SessionState,
        TurnProjection, TurnTopology,
    },
    error::{Error, Result},
};
use pontia_storage_sqlite::repositories::{
    events::SqliteEventRepository, sessions::SqliteSessionRepository, turns::SqliteTurnRepository,
};

use self::{
    effects::{clear_exited_session_tmux_markers, link_started_turn_to_inbox_message},
    enrichment::{
        consume_transient_pi_native_evidence, enrich_pi_topology, enrich_timeline_boundary,
        should_resolve_pi_topology,
    },
    persistence::{insert_event_in_tx, persist_projections_in_tx},
    validation::{
        ensure_confirmed_event_matches_session_boundary, ensure_runtime_fence_in_tx,
        validate_turn_identity_in_tx,
    },
};
use super::{
    EventIngestResult, PontiaEvent,
    projection_rows::{event_from_row, session_from_row, turn_from_row},
};
use crate::{AgentEventBroker, InboxCommandService, UpsertAgentBindingRequest};

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
        self.ingest_domain_event(event.into_reported_event().into(), None, false, None)
            .await
            .map(|result| result.expect("unconditional event ingestion returns a result"))
    }

    pub(crate) async fn ingest_startup_timeout_event(&self, event: PontiaEvent) -> Result<bool> {
        Ok(self
            .ingest_domain_event(
                event.into_reported_event().into(),
                None,
                false,
                Some(SessionState::Starting),
            )
            .await?
            .is_some())
    }

    /// Ingests a Pontia runtime observation while fencing it to the currently
    /// bound runtime instance.
    pub async fn ingest_runtime_observation_event(
        &self,
        event: PontiaEvent,
    ) -> Result<EventIngestResult> {
        self.ingest_domain_event(event.into_reported_event().into(), None, true, None)
            .await
            .map(|result| result.expect("unconditional event ingestion returns a result"))
    }

    /// Ingests a fact supplied by an explicit agent-client adapter.
    ///
    /// This path preserves adapter and replay behavior that predates runtime
    /// fencing. HTTP reports must use [`Self::ingest_confirmed_event`], while
    /// Pontia-owned callers must use [`Self::ingest_pontia_event`].
    pub async fn ingest_reported_event(&self, event: ReportedEvent) -> Result<EventIngestResult> {
        self.ingest_domain_event(event.into(), None, false, None)
            .await
            .map(|result| result.expect("unconditional event ingestion returns a result"))
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
        self.ingest_domain_event(event.into(), None, true, None)
            .await
            .map(|result| result.expect("unconditional event ingestion returns a result"))
    }

    pub async fn ingest_event_with_topology(
        &self,
        event: ReportedEvent,
        topology: TurnTopology,
    ) -> Result<EventIngestResult> {
        let mut event: DomainEvent = event.into();
        event.topology = Some(topology);
        self.ingest_domain_event(event, None, false, None)
            .await
            .map(|result| result.expect("unconditional event ingestion returns a result"))
    }

    pub(crate) async fn ingest_pontia_event_with_agent_binding(
        &self,
        event: PontiaEvent,
        binding: UpsertAgentBindingRequest,
    ) -> Result<EventIngestResult> {
        self.ingest_domain_event(
            event.into_reported_event().into(),
            Some(binding),
            false,
            None,
        )
        .await
        .map(|result| result.expect("unconditional event ingestion returns a result"))
    }

    async fn ingest_domain_event(
        &self,
        mut event: DomainEvent,
        initial_agent_binding: Option<UpsertAgentBindingRequest>,
        enforce_runtime_fence: bool,
        expected_session_state: Option<SessionState>,
    ) -> Result<Option<EventIngestResult>> {
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
            clear_exited_session_tmux_markers(&self.pool, &event, false).await;
            return Ok(Some(EventIngestResult {
                accepted: true,
                duplicate: true,
                event_id: event.event_id,
                session_id: event.session_id,
                turn_id: event.turn_id,
                state_version: existing_version,
            }));
        }

        enrich_timeline_boundary(&self.pool, &mut event).await;
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
                ensure_runtime_fence_in_tx(&mut tx, &event).await?;
            }
        }
        validate_turn_identity_in_tx(&mut tx, &event, enforce_runtime_fence).await?;
        let sessions =
            SqliteSessionRepository::load_projection_rows_in_tx(&mut tx, &event.session_id)
                .await?
                .into_iter()
                .map(session_from_row)
                .collect::<Result<Vec<_>>>()?;
        if let Some(expected_state) = expected_session_state
            && !sessions
                .first()
                .is_some_and(|session| session.state == expected_state)
        {
            return Ok(None);
        }
        let turns = SqliteTurnRepository::load_projection_rows_in_tx(&mut tx, &event.session_id)
            .await?
            .into_iter()
            .map(turn_from_row)
            .collect::<Result<Vec<_>>>()?;
        enrich_pi_topology(&mut event, topology_binding_id, topology_evidence, &turns);
        let mut projection = ProjectionState::with_existing(sessions, turns);
        projection.apply(&event)?;

        insert_event_in_tx(&mut tx, &event).await?;

        let state_version =
            SqliteEventRepository::session_event_count_in_tx(&mut tx, &event.session_id).await?;

        if event.event_type != EventType::SessionMessageUpdated {
            persist_projections_in_tx(&mut tx, &projection, state_version).await?;
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

        clear_exited_session_tmux_markers(&self.pool, &event, true).await;
        link_started_turn_to_inbox_message(&self.pool, &event).await?;

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

        Ok(Some(EventIngestResult {
            accepted: true,
            duplicate: false,
            event_id: event.event_id,
            session_id: event.session_id,
            turn_id: event.turn_id,
            state_version,
        }))
    }

    pub async fn ensure_confirmed_event_matches_session_boundary(
        &self,
        event: &DomainEvent,
    ) -> Result<()> {
        ensure_confirmed_event_matches_session_boundary(&self.pool, event).await
    }

    pub async fn get_session(&self, session_id: &str) -> Result<Option<SessionProjection>> {
        let mut sessions = self.load_session_projection(session_id).await?;
        Ok(sessions.pop())
    }

    pub async fn get_turn(&self, turn_id: &str) -> Result<Option<TurnProjection>> {
        SqliteTurnRepository::new(self.pool.clone())
            .get_projection(turn_id)
            .await?
            .map(turn_from_row)
            .transpose()
    }

    pub async fn list_events(&self, session_id: &str) -> Result<Vec<DomainEvent>> {
        let rows = SqliteEventRepository::new(self.pool.clone())
            .list_domain_event_rows(session_id)
            .await?;

        rows.into_iter().map(event_from_row).collect()
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

        rows.into_iter().map(session_from_row).collect()
    }
}
