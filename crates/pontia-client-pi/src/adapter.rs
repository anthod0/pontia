use crate::launch::PiLauncher;
use crate::raw_transcripts::{PiAgentBindingResolver, PiTimelineAdapter};
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
        service: Some(Arc::new(crate::service::PiService)),
        profile: None,
        events: Some(Arc::new(crate::lifecycle::PiEventInterpreter)),
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
    fn native_history(
        &self,
        binding: pontia_application::client_contract::raw_transcripts::AgentBindingResolveRequest,
        control: Option<pontia_application::clients::ClientControlService>,
    ) -> Option<Arc<dyn pontia_application::client_contract::native_history::NativeHistory>> {
        Some(crate::history::PiHistory::new(binding, control))
    }
    fn capture_native_boundary(
        &self,
        binding: &pontia_application::client_contract::raw_transcripts::AgentBindingResolveRequest,
        kind: pontia_application::client_contract::raw_transcripts::TimelineBoundaryCaptureKind,
        anchor: Option<String>,
        first: bool,
        head: Option<&str>,
    ) -> Option<
        Result<pontia_application::client_contract::raw_transcripts::CapturedTimelineBoundary>,
    > {
        use pontia_application::client_contract::raw_transcripts::{
            CapturedTimelineBoundary, TimelineBoundaryCaptureKind, TimelineBoundaryCaptureRequest,
            TimelineBoundaryCapturer,
        };
        Some((|| {
            if kind == TimelineBoundaryCaptureKind::Tail
                && head.is_some_and(|h| !crate::history::PiEntryCursor::is_entry(h))
            {
                let head = head.unwrap();
                crate::raw_transcripts::PiJsonlV2Cursor::decode(head, &binding.id)?;
                use pontia_application::client_contract::raw_transcripts::AgentBindingResolver;
                let source = PiAgentBindingResolver::new().resolve(binding)?;
                return PiTimelineAdapter::new().capture_boundary(TimelineBoundaryCaptureRequest {
                    source,
                    kind,
                    native_entry_anchor: anchor,
                    allow_missing_native_entry_anchor: false,
                });
            }
            if let Some(head) = head {
                crate::history::PiEntryCursor::decode(
                    head,
                    &binding.id,
                    Some(&binding.client_session_key),
                )?;
            }
            if anchor
                .as_deref()
                .is_some_and(|id| id.trim().is_empty() || id.len() > 512)
                || (anchor.is_none() && !(first && kind == TimelineBoundaryCaptureKind::Head))
            {
                return Err(Error::Domain(
                    "cursor_invalid: native entry anchor required".into(),
                ));
            }
            if binding.client_session_key.trim().is_empty() {
                return Err(Error::Domain(
                    "cursor_invalid: native Session identity missing".into(),
                ));
            }
            Ok(CapturedTimelineBoundary {
                kind,
                cursor: crate::history::PiEntryCursor {
                    binding_id: binding.id.clone(),
                    session_id: binding.client_session_key.clone(),
                    anchor,
                    relation: "after".into(),
                }
                .encode(),
            })
        })())
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
    fn history_recovery(
        &self,
    ) -> Option<
        Box<dyn pontia_application::client_contract::history::TurnHistoryRecoverer + Send + Sync>,
    > {
        Some(Box::new(PiTimelineAdapter::new()))
    }
    fn branch_target(&self, _request: BranchTargetRequest) -> Result<String> {
        Err(Error::CapabilityUnavailable(
            "Pi branch target requires an asynchronous history source".into(),
        ))
    }
}
