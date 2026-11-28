use super::{TimelineBoundaryBackend, TurnTimelineBackend, TurnTopologyBackend};
use pontia_core::{
    Result,
    domain::{DomainEvent, EventType},
};
use pontia_runtime::RuntimeStartRequest;
use serde_json::Value;
use std::path::Path;

#[derive(Default)]
pub struct NativeEventEvidence {
    pub entry_anchor: Option<String>,
    pub topology: Option<Value>,
}

pub struct BranchTargetRequest {
    pub binding: pontia_storage_sqlite::models::agent_bindings::AgentBindingRow,
    pub turn: pontia_storage_sqlite::models::turns::TurnProjectionRow,
    pub is_first_session_turn: bool,
}

#[derive(Clone, Copy)]
pub struct TimelineCachePolicy {
    pub render_version: i64,
    pub read_while_online: bool,
    pub backfill_all_turns_on_miss: bool,
    pub supports_session_checkpoints: bool,
    pub preserve_pre_compaction: bool,
}

pub struct ResidentTimeline {
    pub history: std::sync::Arc<dyn super::native_history::NativeHistory>,
    pub session_checkpoints: bool,
}

/// Native parsing operates on opaque client evidence; business identity stays with services.
pub trait ClientData: Send + Sync {
    /// History support precedes setup. Native binding and Turn heads arrive with lifecycle facts.
    fn timeline_uses_lifecycle_anchors(&self) -> bool {
        false
    }

    /// Render format of terminal Turns eligible for persistent timeline caching.
    fn timeline_cache_policy(&self) -> Option<TimelineCachePolicy> {
        None
    }

    /// A cheap fingerprint for adapters backed by native files.
    fn timeline_cache_validator(
        &self,
        binding: &super::raw_transcripts::AgentBindingResolveRequest,
        _source: Option<super::native_history::HistorySource>,
    ) -> Result<Option<String>> {
        let Some(path) = &binding.client_session_file else {
            return Ok(None);
        };
        let meta = std::fs::metadata(path)
            .map_err(|e| pontia_core::Error::Unavailable(format!("{}: {e}", path.display())))?;
        let modified = meta
            .modified()?
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(std::io::Error::other)?;
        Ok(Some(serde_json::to_string(&(
            path,
            meta.len(),
            modified.as_nanos(),
        ))?))
    }

    /// A resident snapshot provider for eager writes. It must never perform native replay.
    fn resident_history(
        &self,
        _binding: super::raw_transcripts::AgentBindingResolveRequest,
    ) -> Option<ResidentTimeline> {
        None
    }

    fn normalize_payload(&self, kind: EventType, data: Value) -> Result<Value>;
    fn take_evidence(&self, event: &mut DomainEvent) -> NativeEventEvidence;
    /// Clients whose native history availability is instance-specific can verify it on demand.
    fn probe_timeline(
        &self,
        _exited: bool,
        _binding: &super::raw_transcripts::AgentBindingResolveRequest,
    ) -> Option<Result<()>> {
        None
    }
    /// Optional identity-based capture. None retains the client's file boundary backend.
    fn capture_native_boundary(
        &self,
        _binding: &super::raw_transcripts::AgentBindingResolveRequest,
        _request: super::raw_transcripts::TimelineBoundaryCaptureKind,
        _anchor: Option<String>,
        _first_turn: bool,
        _head: Option<&str>,
    ) -> Option<Result<super::raw_transcripts::CapturedTimelineBoundary>> {
        None
    }
    fn timeline(&self) -> Option<TurnTimelineBackend> {
        None
    }
    fn boundaries(&self) -> Option<TimelineBoundaryBackend> {
        None
    }
    fn topology(&self) -> Option<TurnTopologyBackend>;
    fn history_recovery(
        &self,
    ) -> Option<Box<dyn super::history::TurnHistoryRecoverer + Send + Sync>> {
        None
    }
    fn branch_target(&self, request: BranchTargetRequest) -> Result<String>;
}

pub struct ClientLaunchRequest<'a> {
    pub root: &'a Path,
    pub runtime: RuntimeStartRequest,
    pub restart_count: i64,
    pub reuse_pane: Option<(&'a str, &'a str)>,
    pub native_session_key: Option<&'a str>,
    pub native_session_file: Option<&'a Path>,
}
