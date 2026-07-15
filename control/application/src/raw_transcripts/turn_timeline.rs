use serde::Serialize;
use sqlx::SqlitePool;

use pontia_agent_clients::{
    self as agent_clients,
    raw_transcripts::{
        AgentBindingResolveRequest, TurnTimelineItem, TurnTimelineRange, TurnTimelineReadError,
        TurnTimelineReadRequest,
    },
};
use pontia_core::{domain::TurnState, error::Error};
use pontia_storage_sqlite::repositories::turns::SqliteTurnRepository;

use crate::{AgentBindingService, ExternalQueryService};

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

#[derive(Debug)]
pub enum TurnTimelineServiceError {
    SessionNotFound,
    TurnNotFound,
    CapabilityUnavailable,
    TurnUnavailable { turn_id: String },
    TimelineInvalid { turn_id: String },
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
    pool: SqlitePool,
}

impl TurnTimelineService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn page(
        &self,
        session_id: String,
        direction: TurnTimelineDirection,
        anchor_turn_id: Option<String>,
        limit: usize,
    ) -> Result<TurnTimelinePage, TurnTimelineServiceError> {
        let Some(session) = ExternalQueryService::new(self.pool.clone())
            .get_session(&session_id)
            .await?
        else {
            return Err(TurnTimelineServiceError::SessionNotFound);
        };

        let turns = SqliteTurnRepository::new(self.pool.clone())
            .list_turns(&session_id)
            .await?;
        if turns.is_empty() {
            return Ok(TurnTimelinePage {
                session_id,
                direction,
                items: Vec::new(),
                next_turn_id: None,
            });
        }

        let anchor_index = anchor_turn_id
            .as_deref()
            .map(|anchor| {
                turns
                    .iter()
                    .position(|turn| turn.turn_id == anchor)
                    .ok_or(TurnTimelineServiceError::TurnNotFound)
            })
            .transpose()?;
        let directional = match direction {
            TurnTimelineDirection::Forward => turns
                .iter()
                .skip(anchor_index.unwrap_or(0))
                .collect::<Vec<_>>(),
            TurnTimelineDirection::Backward => turns
                .iter()
                .take(anchor_index.map_or(turns.len(), |index| index + 1))
                .rev()
                .collect::<Vec<_>>(),
        };
        let next_turn_id = directional.get(limit).map(|turn| turn.turn_id.clone());
        let mut selected = directional.into_iter().take(limit).collect::<Vec<_>>();
        selected.sort_by_key(|turn| turn.turn_index);

        let newest_turn_id = turns.last().map(|turn| turn.turn_id.as_str());
        let mut ranges = Vec::with_capacity(selected.len());
        for turn in selected {
            let turn_state = turn.state.parse::<TurnState>()?;
            let Some(head_cursor) = turn.head_cursor.clone() else {
                return Err(TurnTimelineServiceError::TurnUnavailable {
                    turn_id: turn.turn_id.clone(),
                });
            };
            let tail_cursor = match turn.tail_cursor.clone() {
                Some(tail_cursor) => Some(tail_cursor),
                None if session.current_turn_id.as_deref() == Some(turn.turn_id.as_str())
                    && newest_turn_id == Some(turn.turn_id.as_str())
                    && turn_state.is_active() =>
                {
                    None
                }
                None => {
                    return Err(TurnTimelineServiceError::TurnUnavailable {
                        turn_id: turn.turn_id.clone(),
                    });
                }
            };
            ranges.push(TurnTimelineRange {
                turn_id: turn.turn_id.clone(),
                turn_index: turn.turn_index,
                head_cursor,
                tail_cursor,
            });
        }

        let binding = AgentBindingService::new(self.pool.clone())
            .binding_for_session(&session_id)
            .await?
            .ok_or(TurnTimelineServiceError::CapabilityUnavailable)?;
        let backend = agent_clients::turn_timeline_backend_for(&binding.client_type)
            .ok_or(TurnTimelineServiceError::CapabilityUnavailable)?;
        let source = backend
            .resolver
            .resolve(&AgentBindingResolveRequest {
                id: binding.id,
                session_id: binding.session_id,
                client_type: binding.client_type,
                launch_cwd: binding.launch_cwd.into(),
                client_session_key: binding.client_session_key,
            })
            .map_err(classify_adapter_error)?;
        let items = backend
            .reader
            .read_turn_ranges(TurnTimelineReadRequest { source, ranges })
            .map_err(classify_reader_error)?;

        Ok(TurnTimelinePage {
            session_id,
            direction,
            items,
            next_turn_id,
        })
    }
}

fn classify_adapter_error(error: Error) -> TurnTimelineServiceError {
    let message = error.to_string();
    if message.contains("source_unavailable:") {
        return TurnTimelineServiceError::SourceUnavailable;
    }
    match error {
        Error::CapabilityUnavailable(_) => TurnTimelineServiceError::CapabilityUnavailable,
        error => TurnTimelineServiceError::Inner(error),
    }
}

fn classify_reader_error(error: TurnTimelineReadError) -> TurnTimelineServiceError {
    match error {
        TurnTimelineReadError::InvalidRange { turn_id, .. } => {
            TurnTimelineServiceError::TimelineInvalid { turn_id }
        }
        TurnTimelineReadError::Inner(error) => classify_adapter_error(error),
    }
}
