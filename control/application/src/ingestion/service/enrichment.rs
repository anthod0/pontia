use std::path::PathBuf;

use serde_json::Value;

use pontia_agent_clients::raw_transcripts::{
    AgentBindingResolveRequest, TimelineBoundaryCaptureKind, TimelineBoundaryCaptureRequest,
};
use pontia_agent_clients::{
    TopologyDiagnostic, TopologyResolution, TopologyResolveRequest, TurnTopologyCandidate,
};
use pontia_core::{
    domain::{DomainEvent, EventSource, EventType, TimelineBoundary, TurnProjection, TurnTopology},
    error::Error,
};

use super::EventIngestService;

impl EventIngestService {
    pub(super) fn enrich_pi_topology(
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

    pub(super) async fn enrich_timeline_boundary(&self, event: &mut DomainEvent) {
        let Some(kind) = timeline_boundary_kind(event.event_type) else {
            return;
        };
        if event.source != EventSource::AgentAdapter {
            return;
        }
        let Some(backend) = pontia_agent_clients::timeline_boundary_backend_for(&event.client_type)
        else {
            return;
        };

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
            client_session_file: binding.client_session_file.clone().map(PathBuf::from),
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
}

pub(super) fn should_resolve_pi_topology(event: &DomainEvent) -> bool {
    event.event_type == EventType::TurnStarted
        && event.client_type == "pi"
        && event.source == EventSource::AgentAdapter
}

pub(super) fn consume_transient_pi_native_evidence(event: &mut DomainEvent) -> Option<Value> {
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
