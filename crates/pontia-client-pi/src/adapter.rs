use crate::launch::PiLauncher;
use crate::raw_transcripts::{
    PiAgentBindingResolver, PiTimelineAdapter, PiTurnUserEntryResolveRequest,
    PiTurnUserEntryResolver,
};
use pontia_application::client_contract::{
    TimelineBoundaryBackend, TurnTimelineBackend, TurnTopologyBackend,
};
use pontia_application::clients::{
    BranchTargetRequest, ClientData, ClientRegistration, NativeEventEvidence,
};
use pontia_core::{
    Error, Result,
    domain::{DomainEvent, EventType},
};
use serde_json::Value;
use std::sync::Arc;

pub fn registration(tui_command: Option<String>) -> ClientRegistration {
    ClientRegistration {
        in_process: None,
        session: None,
        prepare_on_input: false,
        steer: false,
        spec: &crate::SPEC,
        data: Some(Arc::new(PiData)),
        launcher: Some(Arc::new(PiLauncher { tui_command })),
    }
}

struct PiData;
impl ClientData for PiData {
    fn normalize_payload(&self, kind: EventType, data: Value) -> Result<Value> {
        crate::facts::normalize_payload(kind, data)
    }
    fn take_evidence(&self, event: &mut DomainEvent) -> NativeEventEvidence {
        let entry_anchor = match event.event_type {
            EventType::TurnStarted => event.payload.pointer("/timeline_anchor/previous_leaf_id"),
            _ => event.payload.pointer("/timeline_anchor/terminal_leaf_id"),
        }
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
        let topology = event.payload.as_object_mut().and_then(|payload| {
            payload.remove("timeline_anchor");
            payload.remove("topology_context")
        });
        NativeEventEvidence {
            entry_anchor,
            topology,
        }
    }
    fn timeline(&self) -> TurnTimelineBackend {
        TurnTimelineBackend {
            resolver: Box::new(PiAgentBindingResolver::new()),
            reader: Box::new(PiTimelineAdapter::new()),
        }
    }
    fn boundaries(&self) -> TimelineBoundaryBackend {
        TimelineBoundaryBackend {
            resolver: Box::new(PiAgentBindingResolver::new()),
            capturer: Box::new(PiTimelineAdapter::new()),
        }
    }
    fn topology(&self) -> Option<TurnTopologyBackend> {
        Some(TurnTopologyBackend {
            resolver: Box::new(crate::topology::PiTopologyResolver::new()),
        })
    }
    fn branch_target(&self, request: BranchTargetRequest) -> Result<String> {
        let binding = request.binding;
        let source = self
            .timeline()
            .resolver
            .resolve(
                &pontia_application::client_contract::raw_transcripts::AgentBindingResolveRequest {
                    client_session_key: binding.client_session_key.clone(),
                    id: binding.id,
                    session_id: binding.session_id.clone(),
                    client_type: binding.client_type,
                    client_session_file: binding.client_session_file.map(Into::into),
                },
            )
            .map_err(|error| {
                Error::StateConflict(format!("Pi branch target source unavailable: {error}"))
            })?;
        PiTimelineAdapter::new()
            .resolve_user_entry(PiTurnUserEntryResolveRequest {
                source,
                session_id: binding.session_id,
                turn_session_id: request.turn.session_id,
                turn_id: request.turn.turn_id,
                is_first_session_turn: request.is_first_session_turn,
                head_cursor: request.turn.head_cursor,
                tail_cursor: request.turn.tail_cursor,
            })
            .map(|resolved| resolved.entry_id)
            .map_err(|error| Error::StateConflict(error.to_string()))
    }
}
