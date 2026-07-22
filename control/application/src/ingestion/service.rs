use std::path::PathBuf;

use serde_json::Value;
use sqlx::SqlitePool;

use pontia_core::{
    domain::{
        DomainEvent, EventSource, EventType, ProjectionState, ReportedEvent, SessionProjection,
        TimelineBoundary, TurnProjection, TurnTopology,
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

use super::{EventIngestResult, PontiaEvent};
use crate::{
    InboxCommandService, UpsertAgentBindingRequest, row_to_event, row_to_session, row_to_turn,
};
use pontia_agent_clients::raw_transcripts::{
    AgentBindingResolveRequest, TimelineBoundaryCaptureKind, TimelineBoundaryCaptureRequest,
};
use pontia_agent_clients::{
    TopologyDiagnostic, TopologyResolution, TopologyResolveRequest, TurnTopologyCandidate,
};

#[derive(Clone)]
pub struct EventIngestService {
    pool: SqlitePool,
}

impl EventIngestService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn ingest_pontia_event(&self, event: PontiaEvent) -> Result<EventIngestResult> {
        self.ingest_domain_event(event.into_reported_event().into(), None, false)
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

    fn enrich_pi_topology(
        &self,
        event: &mut DomainEvent,
        binding_id: Option<String>,
        evidence: Option<Value>,
        turns: &[TurnProjection],
    ) {
        if !should_resolve_pi_topology(event) {
            return;
        }
        let Some(turn_id) = event.turn_id.clone() else {
            return;
        };
        let Some(binding_id) = binding_id else {
            event.topology = Some(TurnTopology::Unknown);
            warn_topology_resolution(event, TopologyDiagnostic::BindingUnavailable);
            return;
        };
        let Some(backend) = pontia_agent_clients::topology_backend_for(&event.client_type) else {
            event.topology = Some(TurnTopology::Unknown);
            warn_topology_resolution(event, TopologyDiagnostic::AdapterUnavailable);
            return;
        };
        let earlier_turns = turns
            .iter()
            .filter(|turn| turn.turn_id.as_str() < turn_id.as_str())
            .map(|turn| TurnTopologyCandidate {
                turn_id: turn.turn_id.clone(),
                tail_cursor: turn.tail_cursor.clone(),
            })
            .collect::<Vec<_>>();
        let result = backend.resolver.resolve(TopologyResolveRequest {
            binding_id,
            current_turn_id: turn_id.clone(),
            earlier_turns,
            evidence,
        });
        event.topology = Some(match result.resolution {
            TopologyResolution::Unknown => TurnTopology::Unknown,
            TopologyResolution::Root => TurnTopology::Root,
            TopologyResolution::Linked { parent_turn_id }
                if turns.iter().any(|candidate| {
                    candidate.turn_id == parent_turn_id
                        && candidate.session_id == event.session_id
                        && candidate.turn_id.as_str() < turn_id.as_str()
                }) =>
            {
                TurnTopology::linked(parent_turn_id)
            }
            TopologyResolution::Linked { .. } => {
                warn_topology_resolution(event, TopologyDiagnostic::ParentNotFound);
                TurnTopology::Unknown
            }
        });
        if !matches!(
            result.diagnostic,
            TopologyDiagnostic::RootContext | TopologyDiagnostic::ParentMatched
        ) {
            warn_topology_resolution(event, result.diagnostic);
        }
    }

    async fn enrich_timeline_boundary(&self, event: &mut DomainEvent) {
        let Some(kind) = timeline_boundary_kind(event.event_type) else {
            return;
        };
        if event.client_type != "pi" || event.source != EventSource::AgentAdapter {
            return;
        }

        let turn_id = event.turn_id.as_deref().expect("validated turn_id");
        let binding = match crate::AgentBindingService::new(self.pool.clone())
            .binding_for_session(&event.session_id)
            .await
        {
            Ok(Some(binding)) => binding,
            Ok(None) => {
                warn_timeline_capture_failure(event, turn_id, None, "agent_binding_missing");
                return;
            }
            Err(error) => {
                warn_timeline_capture_failure(
                    event,
                    turn_id,
                    None,
                    &safe_timeline_adapter_error(&error),
                );
                return;
            }
        };

        let Some(backend) = pontia_agent_clients::timeline_boundary_backend_for(&event.client_type)
        else {
            warn_timeline_capture_failure(event, turn_id, Some(&binding.id), "adapter_unavailable");
            return;
        };
        let native_entry_anchor = match kind {
            TimelineBoundaryCaptureKind::Head => {
                event.payload.pointer("/timeline_anchor/previous_leaf_id")
            }
            TimelineBoundaryCaptureKind::Tail => {
                event.payload.pointer("/timeline_anchor/terminal_leaf_id")
            }
        }
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);
        let is_first_session_turn = if kind == TimelineBoundaryCaptureKind::Head {
            match sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM turns WHERE session_id = ? AND turn_id <> ?",
            )
            .bind(&event.session_id)
            .bind(turn_id)
            .fetch_one(&self.pool)
            .await
            {
                Ok(count) => count == 0,
                Err(_) => false,
            }
        } else {
            false
        };
        let source = match backend.resolver.resolve(&AgentBindingResolveRequest {
            id: binding.id.clone(),
            session_id: binding.session_id.clone(),
            client_type: binding.client_type.clone(),
            launch_cwd: PathBuf::from(&binding.launch_cwd),
            client_session_key: binding.client_session_key.clone(),
        }) {
            Ok(source) => source,
            Err(error) => {
                let adapter_error = safe_timeline_adapter_error(&error);
                if is_first_session_turn && adapter_error == "source_unavailable" {
                    match backend
                        .capturer
                        .capture_source_origin_head(&binding.id, native_entry_anchor.clone())
                    {
                        Ok(boundary) => {
                            event.timeline_boundary = Some(TimelineBoundary::head(boundary.cursor));
                        }
                        Err(error) => warn_timeline_capture_failure(
                            event,
                            turn_id,
                            Some(&binding.id),
                            &safe_timeline_adapter_error(&error),
                        ),
                    }
                    return;
                }
                warn_timeline_capture_failure(event, turn_id, Some(&binding.id), &adapter_error);
                return;
            }
        };

        match backend
            .capturer
            .capture_boundary(TimelineBoundaryCaptureRequest {
                source,
                kind,
                native_entry_anchor,
                allow_missing_native_entry_anchor: is_first_session_turn,
            }) {
            Ok(boundary) => {
                event.timeline_boundary = Some(match boundary.kind {
                    TimelineBoundaryCaptureKind::Head => TimelineBoundary::head(boundary.cursor),
                    TimelineBoundaryCaptureKind::Tail => TimelineBoundary::tail(boundary.cursor),
                });
            }
            Err(error) => warn_timeline_capture_failure(
                event,
                turn_id,
                Some(&binding.id),
                &safe_timeline_adapter_error(&error),
            ),
        }
    }

    async fn validate_turn_identity_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        event: &DomainEvent,
        require_existing_followup: bool,
    ) -> Result<()> {
        if !event.event_type.is_turn_event() {
            return Ok(());
        }
        let turn_id = event.turn_id.as_deref().expect("validated turn_id");

        match SqliteTurnRepository::turn_session_id_in_tx(tx, turn_id).await? {
            Some(session_id) if session_id != event.session_id => Err(Error::Domain(format!(
                "turn {turn_id} belongs to session {session_id}, not {}",
                event.session_id
            ))),
            Some(_) => Ok(()),
            None if event_type_can_create_turn(event.event_type) || !require_existing_followup => {
                Ok(())
            }
            None => Err(Error::Domain(format!(
                "{} references unknown turn {turn_id} in session {}",
                event.event_type, event.session_id
            ))),
        }
    }

    async fn ensure_runtime_fence_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        event: &DomainEvent,
    ) -> Result<()> {
        if !is_confirmed_runtime_source(event.source)
            || !runtime_instance_id_required_for_event(event.event_type)
        {
            return Ok(());
        }
        let expected_runtime_instance_id =
            SqliteRuntimeBindingRepository::runtime_instance_id_in_tx(tx, &event.session_id)
                .await?;
        let Some(expected_runtime_instance_id) = expected_runtime_instance_id else {
            if event.event_type == EventType::SessionReady {
                return Err(Error::Domain(format!(
                    "{} from {} requires a confirmed Runtime binding for session {}",
                    event.event_type, event.source, event.session_id
                )));
            }
            return Ok(());
        };
        let provided_runtime_instance_id = event
            .payload
            .get("runtime_instance_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                Error::Domain(format!(
                    "{} from {} requires payload.runtime_instance_id for runtime-bound session {}",
                    event.event_type, event.source, event.session_id
                ))
            })?;
        if provided_runtime_instance_id != expected_runtime_instance_id {
            return Err(Error::Domain(format!(
                "payload.runtime_instance_id does not match session {} runtime binding",
                event.session_id
            )));
        }
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
            if event.event_type == EventType::SessionReady {
                return Err(Error::Domain(format!(
                    "{} from {} requires a confirmed Runtime binding for session {}",
                    event.event_type, event.source, event.session_id
                )));
            }
            return Ok(());
        };

        let provided_runtime_instance_id = event
            .payload
            .get("runtime_instance_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty());

        if runtime_instance_id_required_for_event(event.event_type) {
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

fn should_resolve_pi_topology(event: &DomainEvent) -> bool {
    event.event_type == EventType::TurnStarted
        && event.client_type == "pi"
        && event.source == EventSource::AgentAdapter
}

fn consume_transient_pi_native_evidence(event: &mut DomainEvent) -> Option<Value> {
    if event.client_type != "pi" || event.source != EventSource::AgentAdapter {
        return None;
    }
    let payload = event.payload.as_object_mut()?;
    let topology_evidence = payload.remove("topology_context");
    payload.remove("timeline_anchor");
    topology_evidence
}

fn warn_topology_resolution(event: &DomainEvent, diagnostic: TopologyDiagnostic) {
    tracing::warn!(
        code = "turn_topology_unresolved",
        event_id = %event.event_id,
        session_id = %event.session_id,
        turn_id = ?event.turn_id,
        client_type = %event.client_type,
        diagnostic = diagnostic.as_str(),
        "Turn topology evidence could not be resolved"
    );
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

fn event_type_can_create_turn(event_type: EventType) -> bool {
    matches!(
        event_type,
        EventType::TurnCreated | EventType::TurnQueued | EventType::TurnStarted
    )
}

fn timeline_boundary_kind(event_type: EventType) -> Option<TimelineBoundaryCaptureKind> {
    match event_type {
        EventType::TurnStarted => Some(TimelineBoundaryCaptureKind::Head),
        EventType::TurnCompleted | EventType::TurnFailed | EventType::TurnInterrupted => {
            Some(TimelineBoundaryCaptureKind::Tail)
        }
        _ => None,
    }
}

fn warn_timeline_capture_failure(
    event: &DomainEvent,
    turn_id: &str,
    binding_id: Option<&str>,
    adapter_error: &str,
) {
    tracing::warn!(
        code = "timeline_boundary_capture_failed",
        event_id = %event.event_id,
        session_id = %event.session_id,
        turn_id,
        event_type = %event.event_type,
        client_type = %event.client_type,
        binding_id = binding_id.unwrap_or("unknown"),
        adapter_error,
        "failed to capture timeline boundary; lifecycle fact will still be persisted"
    );
}

fn safe_timeline_adapter_error(error: &Error) -> String {
    match error {
        Error::Domain(message) => message.clone(),
        Error::CapabilityUnavailable(message) if message.contains("source_unavailable:") => {
            "source_unavailable".to_string()
        }
        Error::CapabilityUnavailable(_) => "capability_unavailable".to_string(),
        Error::Io(error) => format!("io_error:{:?}", error.kind()),
        Error::StateConflict(_) => "state_conflict".to_string(),
        Error::NotFound(_) => "not_found".to_string(),
        _ => "internal_error".to_string(),
    }
}
