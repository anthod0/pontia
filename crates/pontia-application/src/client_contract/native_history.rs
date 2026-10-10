use super::{
    history::{RecoveredTurnHistory, TurnHistoryCandidate},
    raw_transcripts::{TurnTimelineItem, TurnTimelineRange, TurnTimelineReadError},
};
use pontia_core::Result;
use std::{future::Future, pin::Pin};

pub type HistoryRead<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// A request-scoped native source. Implementations own source selection and snapshot reuse.
pub trait NativeHistory: Send + Sync {
    fn read_ranges(
        &self,
        ranges: Vec<TurnTimelineRange>,
    ) -> HistoryRead<'_, std::result::Result<Vec<TurnTimelineItem>, TurnTimelineReadError>>;
    fn recover(
        &self,
        turns: Vec<TurnHistoryCandidate>,
    ) -> HistoryRead<'_, Result<Vec<RecoveredTurnHistory>>>;
    fn branch_target(&self, range: TurnTimelineRange) -> HistoryRead<'_, Result<String>>;
}
