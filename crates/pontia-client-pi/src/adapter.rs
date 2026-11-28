use crate::launch::PiSession;
use crate::raw_transcripts::{PiAgentBindingResolver, PiTimelineAdapter};
use pontia_agent_control::client_contract::{
    TimelineBoundaryBackend, TimelineCachePolicy, TurnTimelineBackend, TurnTopologyBackend,
};
use pontia_agent_control::clients::{
    BranchTargetRequest, ClientData, ClientRegistration, NativeEventEvidence,
};
use pontia_core::{
    Error, Result,
    domain::{DomainEvent, EventType},
};
use serde_json::Value;
use std::sync::Arc;

pub fn registration(tui_command: Option<String>) -> ClientRegistration {
    let connections = crate::connections::PiConnections::default();
    let file = tui_command
        .as_ref()
        .map(|command| serde_json::json!({"tui_command":command}))
        .unwrap_or_else(|| serde_json::json!({}));
    let environment = std::env::var("PONTIA_PI_TUI_COMMAND").ok()
        .map(|command| serde_json::json!({"tui_command": if command.trim().is_empty() { "pi" } else { &command }}))
        .unwrap_or_else(|| serde_json::json!({}));
    let configuration = Arc::new(crate::config::PiConfiguration::new(file, environment));
    ClientRegistration {
        auth: Some(Arc::new(crate::auth::PiAuth::new(configuration.clone()))),
        configuration: Some(configuration),
        interactions: None,
        service: Some(Arc::new(crate::service::PiService {
            connections: connections.clone(),
        })),
        instructions: Some(Arc::new(crate::instructions::InstructionsPolicy)),
        events: Some(Arc::new(crate::lifecycle::PiEventInterpreter)),
        in_process: None,
        session: Some(Arc::new(PiSession {
            tui_command,
            connections: connections.clone(),
        })),
        prepare_on_input: false,
        spec: &crate::SPEC,
        data: Some(Arc::new(PiData { connections })),
        launcher: None,
    }
}

struct PiData {
    connections: crate::connections::PiConnections,
}
impl ClientData for PiData {
    fn timeline_cache_policy(&self) -> Option<TimelineCachePolicy> {
        Some(TimelineCachePolicy {
            render_version: 1,
            read_while_online: false,
            backfill_all_turns_on_miss: false,
            supports_session_checkpoints: true,
            preserve_pre_compaction: false,
        })
    }

    fn timeline_cache_validator(
        &self,
        binding: &pontia_agent_control::client_contract::raw_transcripts::AgentBindingResolveRequest,
        _source: Option<pontia_agent_control::client_contract::native_history::HistorySource>,
    ) -> Result<Option<String>> {
        use std::os::unix::fs::MetadataExt;
        let path = binding.client_session_file.as_ref().ok_or_else(|| {
            Error::CapabilityUnavailable("source_unavailable: missing Pi session file".into())
        })?;
        let file = std::fs::File::open(path).map_err(|error| {
            Error::CapabilityUnavailable(format!("source_unavailable: {}: {error}", path.display()))
        })?;
        let meta = file.metadata()?;
        Ok(Some(serde_json::to_string(&(
            path,
            meta.dev(),
            meta.ino(),
            meta.len(),
            meta.ctime(),
            meta.ctime_nsec(),
        ))?))
    }

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
        let topology = event.payload.get("topology_context").cloned();
        NativeEventEvidence {
            entry_anchor,
            topology,
        }
    }
    fn native_history(
        &self,
        binding: pontia_agent_control::client_contract::raw_transcripts::AgentBindingResolveRequest,
        _control: Option<pontia_agent_control::clients::ClientControlService>,
    ) -> Option<Arc<dyn pontia_agent_control::client_contract::native_history::NativeHistory>> {
        Some(crate::history::PiHistory::new(
            binding,
            self.connections.clone(),
        ))
    }
    fn capture_native_boundary(
        &self,
        binding: &pontia_agent_control::client_contract::raw_transcripts::AgentBindingResolveRequest,
        kind: pontia_agent_control::client_contract::raw_transcripts::TimelineBoundaryCaptureKind,
        anchor: Option<String>,
        first: bool,
        head: Option<&str>,
    ) -> Option<
        Result<pontia_agent_control::client_contract::raw_transcripts::CapturedTimelineBoundary>,
    > {
        use pontia_agent_control::client_contract::raw_transcripts::{
            CapturedTimelineBoundary, TimelineBoundaryCaptureKind, TimelineBoundaryCaptureRequest,
            TimelineBoundaryCapturer,
        };
        Some((|| {
            if kind == TimelineBoundaryCaptureKind::Tail
                && head.is_some_and(|h| !crate::history::PiEntryCursor::is_entry(h))
            {
                let head = head.unwrap();
                crate::raw_transcripts::PiJsonlV2Cursor::decode(head, &binding.id)?;
                use pontia_agent_control::client_contract::raw_transcripts::AgentBindingResolver;
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
        Box<dyn pontia_agent_control::client_contract::history::TurnHistoryRecoverer + Send + Sync>,
    > {
        Some(Box::new(PiTimelineAdapter::new()))
    }
    fn branch_target(&self, _request: BranchTargetRequest) -> Result<String> {
        Err(Error::CapabilityUnavailable(
            "Pi branch target requires an asynchronous history source".into(),
        ))
    }
}
