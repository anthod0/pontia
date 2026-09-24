mod history_recovery;
mod page;
mod recovery;
mod source;
mod topology;
mod tree_history;
mod tree_updates;

use pontia_core::error::Error;
use serde::Serialize;

use crate::client_contract::raw_transcripts::TurnTimelineItem;

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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub history_issue: Option<TurnHistoryIssue>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnHistoryIssue {
    TopologyUnknown,
    RangeUnavailable,
    RangeInvalid,
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
    NativeAssociationUnavailable { turn_id: String },
    TimelineInvalid { turn_id: String },
    TopologyUnknown { turn_id: String },
    TopologyInvalid { turn_id: String },
    SourceUnavailable,
    Pending,
    SourceIdentityMismatch,
    Inner(Error),
}

impl From<Error> for TurnTimelineServiceError {
    fn from(error: Error) -> Self {
        Self::Inner(error)
    }
}

#[derive(Clone)]
pub struct TurnTimelineService {
    clients: crate::clients::ClientRegistry,
    pub(super) pool: sqlx::SqlitePool,
    events: crate::EventIngestService,
}

impl TurnTimelineService {
    pub fn new(events: crate::EventIngestService) -> Self {
        Self {
            pool: events.db(),
            clients: events.clients(),
            events,
        }
    }
}
