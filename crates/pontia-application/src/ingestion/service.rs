mod effects;
mod enrichment;
mod persistence;
mod reporting;
mod reporting_failure;
mod validation;

use sqlx::SqlitePool;

use pontia_core::{
    domain::{
        DomainEvent, EventType, MAX_TURN_INPUT_SUMMARY_CHARS, ProjectionState, ReportedEvent,
        SessionProjection, SessionState, TurnProjection, TurnTopology,
    },
    error::{Error, Result},
};
use pontia_storage_sqlite::repositories::{
    events::SqliteEventRepository, sessions::SqliteSessionRepository, turns::SqliteTurnRepository,
};

use self::{
    effects::clear_exited_session_tmux_markers,
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
use crate::{AgentEventBroker, InboxCommandService, LiveOutputService, UpsertAgentBindingRequest};

#[derive(Clone)]
pub struct EventIngestService {
    pool: SqlitePool,
    inbox_scheduler: crate::inbox::InboxScheduler,
    pi_control: Option<crate::PiControlService>,
    agent_events: Option<AgentEventBroker>,
    live_output: Option<LiveOutputService>,
    volatile_events: Option<crate::app::VolatileEventBroker>,
}

impl EventIngestService {
    pub(crate) fn pi_control(&self) -> Option<crate::PiControlService> {
        self.pi_control.clone()
    }

    pub(crate) fn inbox_scheduler(&self) -> crate::inbox::InboxScheduler {
        self.inbox_scheduler.clone()
    }
    pub(crate) fn control_available(&self, session: &str) {
        InboxCommandService::new(self.clone()).notify_available(session);
    }

    pub fn db(&self) -> SqlitePool {
        self.pool.clone()
    }

    pub fn with_reporting_dependencies(
        mut self,
        pi_control: crate::PiControlService,
        agent_events: AgentEventBroker,
        live_output: LiveOutputService,
        volatile_events: crate::app::VolatileEventBroker,
    ) -> Self {
        self.pi_control = Some(pi_control);
        self.agent_events = Some(agent_events);
        self.live_output = Some(live_output);
        self.volatile_events = Some(volatile_events);
        self
    }

    pub fn with_pi_control(mut self, pi_control: crate::PiControlService) -> Self {
        self.pi_control = Some(pi_control);
        self
    }

    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            inbox_scheduler: Default::default(),
            pi_control: None,
            agent_events: None,
            live_output: None,
            volatile_events: None,
        }
    }

    pub fn with_agent_events(mut self, agent_events: AgentEventBroker) -> Self {
        self.agent_events = Some(agent_events);
        self
    }

    pub fn with_live_output(mut self, live_output: LiveOutputService) -> Self {
        self.live_output = Some(live_output);
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

    /// Injects an event for storage/projection tests and the in-process generic test client.
    ///
    /// This is a test-support entry point by convention, not by compile-time enforcement.
    /// It bypasses fact normalization, report validation and runtime fencing; it still
    /// runs the shared persistence, projection and configured post-commit effects.
    /// Do not use it for production adapters or add a replay path through it.
    /// Client facts must use [`Self::report_fact`]; Pontia-owned facts must use
    /// [`Self::ingest_pontia_event`] (or [`Self::ingest_runtime_observation_event`]
    /// for runtime observations that require fencing).
    pub async fn ingest_reported_event(&self, event: ReportedEvent) -> Result<EventIngestResult> {
        self.ingest_domain_event(event.into(), None, false, None)
            .await
            .map(|result| result.expect("unconditional event ingestion returns a result"))
    }

    /// Injects ready only for the generic test client; real clients return no event here.
    /// Production client readiness must be reported through [`Self::report_fact`].
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

    /// Persists an already normalized event with transaction-time runtime fencing.
    ///
    /// This is the lower-level persistence step used by [`Self::report_fact`].
    /// "Confirmed" means the caller has already normalized and validated the fact;
    /// this method does not perform the full report validation itself.
    /// Production adapters must call [`Self::report_fact`], even for in-process reports.
    pub async fn ingest_confirmed_event(&self, event: ReportedEvent) -> Result<EventIngestResult> {
        self.ingest_domain_event(event.into(), None, true, None)
            .await
            .map(|result| result.expect("unconditional event ingestion returns a result"))
    }

    /// Injects an event with explicit topology for storage/projection and query tests.
    /// Like [`Self::ingest_reported_event`], this bypasses report validation and runtime
    /// fencing and is not compile-time restricted to tests. Production client facts
    /// must use [`Self::report_fact`], which derives topology through normal ingestion.
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
        // Bound durable input at the shared ingestion boundary, including
        // Pontia-owned and in-process events that do not pass through HTTP.
        if event.event_type.is_turn_event() {
            for pointer in ["/input/summary", "/input_summary"] {
                if let Some(serde_json::Value::String(summary)) = event.payload.pointer_mut(pointer)
                {
                    *summary = summary.chars().take(MAX_TURN_INPUT_SUMMARY_CHARS).collect();
                }
            }
        }
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
        if let Some(event_id) =
            reporting_failure::existing_reporting_failure_in_tx(&mut tx, &event).await?
        {
            let state_version =
                SqliteEventRepository::session_event_count_in_tx(&mut tx, &event.session_id)
                    .await?;
            tx.commit().await?;
            return Ok(Some(EventIngestResult {
                accepted: true,
                duplicate: true,
                event_id,
                session_id: event.session_id,
                turn_id: None,
                state_version,
            }));
        }
        if sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM events WHERE event_id=?")
            .bind(&event.event_id)
            .fetch_one(&mut *tx)
            .await?
            > 0
        {
            let state_version =
                SqliteEventRepository::session_event_count_in_tx(&mut tx, &event.session_id)
                    .await?;
            tx.commit().await?;
            return Ok(Some(EventIngestResult {
                accepted: true,
                duplicate: true,
                event_id: event.event_id,
                session_id: event.session_id,
                turn_id: event.turn_id,
                state_version,
            }));
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

        if event.client_type == "pi"
            && matches!(
                event.event_type,
                EventType::SessionExited | EventType::SessionError
            )
            && let Some(control) = &self.pi_control
        {
            control.refresh_session(&event.session_id).await;
        }

        if let Some(agent_events) = &self.agent_events {
            agent_events.publish(event.clone());
        }
        if let Some(live_output) = &self.live_output {
            match event.event_type {
                EventType::TurnCompleted
                | EventType::TurnFailed
                | EventType::TurnDispatchFailed
                | EventType::TurnAbandoned
                | EventType::TurnInterrupted => {
                    if let Some(turn_id) = event.turn_id.as_deref() {
                        live_output.discard_turn(&event.session_id, turn_id);
                    }
                }
                EventType::SessionExited | EventType::SessionError => {
                    live_output.discard_session(&event.session_id);
                }
                _ => {}
            }
        }

        clear_exited_session_tmux_markers(&self.pool, &event, true).await;
        InboxCommandService::new(self.clone())
            .observe_committed(&event)
            .await?;

        Ok(Some(EventIngestResult {
            accepted: true,
            duplicate: false,
            event_id: event.event_id,
            session_id: event.session_id,
            turn_id: event.turn_id,
            state_version,
        }))
    }

    async fn ensure_confirmed_event_matches_session_boundary(
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

    async fn volatile_state_version(&self, session_id: &str) -> Result<i64> {
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
