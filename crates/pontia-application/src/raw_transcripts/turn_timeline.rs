mod page;
mod source;
mod topology;
mod tree_history;
mod tree_updates;

use pontia_core::error::Error;
use serde::Serialize;
use sqlx::SqlitePool;

use pontia_agent_clients::raw_transcripts::TurnTimelineItem;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TurnTimelineDirection {
    Forward,
    Backward,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TurnTimelinePage {
    pub session_id: String,
    pub direction: TurnTimelineDirection,
    pub items: Vec<TurnTimelineItem>,
    pub next_turn_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TurnTimelineGroup {
    pub turn_id: String,
    pub parent_turn_id: Option<String>,
    pub state: String,
    pub items: Vec<TurnTimelineItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TurnTreeHistoryPage {
    pub session_id: String,
    pub groups: Vec<TurnTimelineGroup>,
    pub next_from_turn_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TurnTreeUpdatesPage {
    pub session_id: String,
    pub current_turn_id: Option<String>,
    pub retain_through_turn_id: Option<String>,
    pub groups: Vec<TurnTimelineGroup>,
}

#[derive(Debug)]
pub enum TurnTimelineServiceError {
    SessionNotFound,
    TurnNotFound,
    CapabilityUnavailable,
    TurnUnavailable { turn_id: String },
    TimelineInvalid { turn_id: String },
    TopologyUnknown { turn_id: String },
    TopologyInvalid { turn_id: String },
    SourceUnavailable,
    Inner(Error),
}

impl From<Error> for TurnTimelineServiceError {
    fn from(error: Error) -> Self {
        Self::Inner(error)
    }
}

#[derive(Clone)]
pub struct TurnTimelineService {
    pub(super) pool: SqlitePool,
}

impl TurnTimelineService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}
