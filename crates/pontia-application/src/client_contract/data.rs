use super::{TimelineBoundaryBackend, TurnTimelineBackend, TurnTopologyBackend};
use pontia_core::{
    Result,
    domain::{DomainEvent, EventType},
};
use pontia_runtime::{RuntimeStartRequest, RuntimeStartResult};
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

/// Native parsing operates on opaque client evidence; business identity stays with services.
pub trait ClientData: Send + Sync {
    fn normalize_payload(&self, kind: EventType, data: Value) -> Result<Value>;
    fn take_evidence(&self, event: &mut DomainEvent) -> NativeEventEvidence;
    /// Clients whose native history availability is instance-specific can verify it on demand.
    fn probe_timeline(
        &self,
        _binding: &super::raw_transcripts::AgentBindingResolveRequest,
    ) -> Option<Result<()>> {
        None
    }
    fn timeline(&self) -> TurnTimelineBackend;
    fn boundaries(&self) -> TimelineBoundaryBackend;
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
}

pub trait ClientLauncher: Send + Sync {
    fn launch(&self, request: ClientLaunchRequest<'_>) -> Result<RuntimeStartResult>;
}
