use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnTopologyCandidate {
    pub turn_id: String,
    pub tail_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TopologyResolveRequest {
    pub binding_id: String,
    pub current_turn_id: String,
    pub earlier_turns: Vec<TurnTopologyCandidate>,
    pub evidence: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TopologyResolution {
    Unknown,
    Root,
    Linked { parent_turn_id: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TopologyDiagnostic {
    RootContext,
    ParentMatched,
    ParentNotFound,
    EvidenceMissing,
    EvidenceInvalid,
    CursorInvalid,
    CandidateBoundaryMissing,
    BindingUnavailable,
    AdapterUnavailable,
}

impl TopologyDiagnostic {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RootContext => "root_context",
            Self::ParentMatched => "parent_matched",
            Self::ParentNotFound => "parent_not_found",
            Self::EvidenceMissing => "evidence_missing",
            Self::EvidenceInvalid => "evidence_invalid",
            Self::CursorInvalid => "cursor_invalid",
            Self::CandidateBoundaryMissing => "candidate_boundary_missing",
            Self::BindingUnavailable => "binding_unavailable",
            Self::AdapterUnavailable => "adapter_unavailable",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopologyResolveResult {
    pub resolution: TopologyResolution,
    pub diagnostic: TopologyDiagnostic,
}

pub trait TurnTopologyResolver {
    fn client_type(&self) -> &'static str;
    fn resolve(&self, request: TopologyResolveRequest) -> TopologyResolveResult;
}
