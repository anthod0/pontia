mod commit;
mod effects;
mod enrichment;
mod persistence;
mod reporting;
mod reporting_failure;
mod validation;

use sqlx::SqlitePool;

use pontia_core::{
    domain::{DomainEvent, ReportedEvent, SessionProjection, SessionState, TurnProjection},
    error::Result,
};
use pontia_storage_sqlite::repositories::{
    events::SqliteEventRepository, sessions::SqliteSessionRepository, turns::SqliteTurnRepository,
};

use self::{
    effects::clear_exited_session_tmux_markers,
    validation::ensure_confirmed_event_matches_session_boundary,
};
use super::{
    EventIngestResult, PontiaEvent,
    projection_rows::{event_from_row, session_from_row, turn_from_row},
};
use crate::UpsertAgentBindingRequest;
#[cfg(any(test, feature = "generic-test-client"))]
use crate::{AgentEventBroker, LiveOutputService};
pub(crate) use effects::PostCommitEffects;
#[cfg(any(test, feature = "generic-test-client"))]
use pontia_core::domain::TurnTopology;

#[derive(Clone)]
pub struct EventIngestService {
    pool: SqlitePool,
    clients: crate::clients::ClientRegistry,
    effects: PostCommitEffects,
}

impl EventIngestService {
    pub fn clients(&self) -> crate::clients::ClientRegistry {
        self.clients.clone()
    }
    #[cfg(any(test, feature = "generic-test-client"))]
    pub fn with_clients(mut self, clients: crate::clients::ClientRegistry) -> Self {
        self.clients = clients;
        self
    }

    /// Notifies the input scheduler that a client channel can accept work.
    pub fn control_available(&self, session: &str) {
        self.effects.scheduler.wake(session.into());
    }

    pub fn db(&self) -> SqlitePool {
        self.pool.clone()
    }

    pub(crate) fn new(
        pool: SqlitePool,
        clients: crate::clients::ClientRegistry,
        effects: PostCommitEffects,
    ) -> Self {
        Self {
            pool,
            clients,
            effects,
        }
    }

    #[cfg(any(test, feature = "generic-test-client"))]
    pub fn for_projection_tests(pool: SqlitePool) -> Self {
        Self {
            pool,
            clients: Default::default(),
            effects: PostCommitEffects::default(),
        }
    }

    #[cfg(any(test, feature = "generic-test-client"))]
    pub fn with_agent_events(mut self, agent_events: AgentEventBroker) -> Self {
        self.effects.agent_events = Some(agent_events);
        self
    }

    #[cfg(any(test, feature = "generic-test-client"))]
    pub fn with_live_output(mut self, live_output: LiveOutputService) -> Self {
        self.effects.live_output = Some(live_output);
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
    /// Available only with test support enabled.
    /// It bypasses fact normalization, report validation and runtime fencing; it still
    /// runs the shared persistence, projection and configured post-commit effects.
    /// Do not use it for production adapters or add a replay path through it.
    /// Client facts must use [`Self::report_fact`]; Pontia-owned facts must use
    /// [`Self::ingest_pontia_event`] (or [`Self::ingest_runtime_observation_event`]
    /// for runtime observations that require fencing).
    #[cfg(any(test, feature = "generic-test-client"))]
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
            self.clients
                .get(client_type)
                .and_then(|entry| entry.in_process.as_ref())
                .map(|client| client.ready(session_id, runtime_instance_id))
        }) else {
            return Ok(());
        };
        self.ingest_domain_event(event.into(), None, false, None)
            .await?;
        Ok(())
    }

    /// Persists an already normalized event with transaction-time runtime fencing.
    ///
    /// This is the lower-level persistence step used by [`Self::report_fact`].
    /// "Confirmed" means the caller has already normalized and validated the fact;
    /// this method does not perform the full report validation itself.
    /// Production adapters must call [`Self::report_fact`], even for in-process reports.
    pub(crate) async fn ingest_confirmed_event(
        &self,
        event: ReportedEvent,
    ) -> Result<EventIngestResult> {
        self.ingest_domain_event(event.into(), None, true, None)
            .await
            .map(|result| result.expect("unconditional event ingestion returns a result"))
    }

    /// Injects an event with explicit topology for storage/projection and query tests.
    /// Like [`Self::ingest_reported_event`], this bypasses report validation and runtime
    /// fencing and requires test support. Production client facts
    /// must use [`Self::report_fact`], which derives topology through normal ingestion.
    #[cfg(any(test, feature = "generic-test-client"))]
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
        event: DomainEvent,
        initial_agent_binding: Option<UpsertAgentBindingRequest>,
        enforce_runtime_fence: bool,
        expected_session_state: Option<SessionState>,
    ) -> Result<Option<EventIngestResult>> {
        let outcome = commit::EventCommitter::new(self.pool.clone(), self.clients.clone())
            .commit(
                event,
                initial_agent_binding,
                enforce_runtime_fence,
                expected_session_state,
            )
            .await?;
        match outcome {
            commit::CommitOutcome::Skipped => Ok(None),
            commit::CommitOutcome::Duplicate { result, cleanup } => {
                if let Some(event) = cleanup {
                    clear_exited_session_tmux_markers(&self.pool, &event, false).await;
                }
                Ok(Some(result))
            }
            commit::CommitOutcome::Committed { result, event } => {
                self.effects.apply(&self.pool, &event).await?;
                Ok(Some(result))
            }
        }
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

    async fn load_session_projection(&self, session_id: &str) -> Result<Vec<SessionProjection>> {
        let rows = SqliteSessionRepository::new(self.pool.clone())
            .load_projection_rows(session_id)
            .await?;

        rows.into_iter().map(session_from_row).collect()
    }
}
