use pontia_core::{
    Result,
    domain::{TurnState, TurnTopology},
};

use super::{TopologyResolveResult, raw_transcripts::ResolvedAgentBinding};

#[derive(Clone)]
pub struct TurnHistoryCandidate {
    pub turn_id: String,
    pub head_cursor: Option<String>,
    pub tail_cursor: Option<String>,
    pub state: TurnState,
    pub topology: TurnTopology,
}

pub struct HistoryRecoveryRequest {
    pub source: ResolvedAgentBinding,
    /// Ordered by Turn identity for validation, never as evidence of ancestry.
    pub turns: Vec<TurnHistoryCandidate>,
}

pub struct RecoveredTurnHistory {
    pub turn_id: String,
    pub tail_cursor: Option<String>,
    pub topology: TopologyResolveResult,
}

pub trait TurnHistoryRecoverer {
    fn recover(&self, request: HistoryRecoveryRequest) -> Result<Vec<RecoveredTurnHistory>>;
}
