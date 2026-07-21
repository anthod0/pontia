use std::collections::{HashMap, HashSet};

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
use pontia_storage_sqlite::{models::turns::TurnRow, repositories::turns::SqliteTurnRepository};

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
        if ExternalQueryService::new(self.pool.clone())
            .get_session(&session_id)
            .await?
            .is_none()
        {
            return Err(TurnTimelineServiceError::SessionNotFound);
        }

        let turn_repository = SqliteTurnRepository::new(self.pool.clone());
        let turns = turn_repository.list_turns(&session_id).await?;
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
        selected.sort_by(|left, right| left.turn_id.cmp(&right.turn_id));
        let items = self
            .read_selected_turns(&session_id, &turns, &selected)
            .await?;

        Ok(TurnTimelinePage {
            session_id,
            direction,
            items,
            next_turn_id,
        })
    }

    pub async fn tree_history(
        &self,
        session_id: String,
        from_turn_id: Option<String>,
        limit: usize,
    ) -> Result<TurnTreeHistoryPage, TurnTimelineServiceError> {
        let session = ExternalQueryService::new(self.pool.clone())
            .get_session(&session_id)
            .await?
            .ok_or(TurnTimelineServiceError::SessionNotFound)?;
        let Some(anchor_turn_id) = from_turn_id.or(session.current_turn_id) else {
            return Ok(TurnTreeHistoryPage {
                session_id,
                groups: Vec::new(),
                next_from_turn_id: None,
            });
        };

        let turns = SqliteTurnRepository::new(self.pool.clone())
            .list_turns(&session_id)
            .await?;
        let by_id = turns_by_id(&turns);
        let mut selected = Vec::with_capacity(limit);
        let mut current_id = anchor_turn_id.as_str();
        let mut visited = HashSet::new();
        let mut next_from_turn_id = None;
        while selected.len() < limit {
            if !visited.insert(current_id) {
                return Err(TurnTimelineServiceError::TopologyInvalid {
                    turn_id: current_id.to_string(),
                });
            }
            let turn = by_id
                .get(current_id)
                .copied()
                .ok_or(TurnTimelineServiceError::TurnNotFound)?;
            selected.push(turn);
            match topology_parent(turn, &by_id)? {
                Some(parent_id) if selected.len() == limit => {
                    next_from_turn_id = Some(parent_id.to_string());
                    break;
                }
                Some(parent_id) => current_id = parent_id,
                None => break,
            }
        }
        selected.reverse();
        let groups = self
            .read_selected_groups(&session_id, &turns, &selected)
            .await?;
        Ok(TurnTreeHistoryPage {
            session_id,
            groups,
            next_from_turn_id,
        })
    }

    pub async fn tree_updates(
        &self,
        session_id: String,
        from_turn_id: Option<String>,
    ) -> Result<TurnTreeUpdatesPage, TurnTimelineServiceError> {
        let session = ExternalQueryService::new(self.pool.clone())
            .get_session(&session_id)
            .await?
            .ok_or(TurnTimelineServiceError::SessionNotFound)?;
        let Some(current_turn_id) = session.current_turn_id else {
            return Ok(TurnTreeUpdatesPage {
                session_id,
                current_turn_id: None,
                retain_through_turn_id: None,
                groups: Vec::new(),
            });
        };

        let turns = SqliteTurnRepository::new(self.pool.clone())
            .list_turns(&session_id)
            .await?;
        let by_id = turns_by_id(&turns);
        let current_chain = ancestor_chain(&current_turn_id, &by_id)?;

        let (retain_through_turn_id, selected) = match from_turn_id {
            None => (None, current_chain.clone()),
            Some(from_turn_id) => {
                if let Some(position) = current_chain
                    .iter()
                    .position(|turn| turn.turn_id == from_turn_id)
                {
                    (Some(from_turn_id), current_chain[position..].to_vec())
                } else {
                    let from_chain = ancestor_chain(&from_turn_id, &by_id)?;
                    let current_ids = current_chain
                        .iter()
                        .enumerate()
                        .map(|(index, turn)| (turn.turn_id.as_str(), index))
                        .collect::<HashMap<_, _>>();
                    let lca = from_chain
                        .iter()
                        .rev()
                        .find_map(|turn| current_ids.get(turn.turn_id.as_str()).copied());
                    match lca {
                        Some(lca_index) => (
                            Some(current_chain[lca_index].turn_id.clone()),
                            current_chain[lca_index + 1..].to_vec(),
                        ),
                        None => (None, current_chain.clone()),
                    }
                }
            }
        };

        let groups = self
            .read_selected_groups(&session_id, &turns, &selected)
            .await?;
        Ok(TurnTreeUpdatesPage {
            session_id,
            current_turn_id: Some(current_turn_id),
            retain_through_turn_id,
            groups,
        })
    }

    async fn read_selected_groups(
        &self,
        session_id: &str,
        all_turns: &[TurnRow],
        selected: &[&TurnRow],
    ) -> Result<Vec<TurnTimelineGroup>, TurnTimelineServiceError> {
        let items = self
            .read_selected_turns(session_id, all_turns, selected)
            .await?;
        let mut items_by_turn: HashMap<String, Vec<TurnTimelineItem>> = HashMap::new();
        for item in items {
            items_by_turn
                .entry(item.turn_id.clone())
                .or_default()
                .push(item);
        }
        Ok(selected
            .iter()
            .map(|turn| TurnTimelineGroup {
                turn_id: turn.turn_id.clone(),
                parent_turn_id: turn.parent_turn_id.clone(),
                state: turn.state.clone(),
                items: items_by_turn.remove(&turn.turn_id).unwrap_or_default(),
            })
            .collect())
    }

    async fn read_selected_turns(
        &self,
        session_id: &str,
        all_turns: &[TurnRow],
        selected: &[&TurnRow],
    ) -> Result<Vec<TurnTimelineItem>, TurnTimelineServiceError> {
        if selected.is_empty() {
            return Ok(Vec::new());
        }

        let active_turn_id = SqliteTurnRepository::new(self.pool.clone())
            .active_turn(session_id)
            .await?
            .map(|turn| turn.turn_id);
        let newest_turn_id = all_turns.last().map(|turn| turn.turn_id.as_str());
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
                None if active_turn_id.as_deref() == Some(turn.turn_id.as_str())
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
                is_first_session_turn: all_turns.first().map(|first| first.turn_id.as_str())
                    == Some(turn.turn_id.as_str()),
                head_cursor,
                tail_cursor,
            });
        }

        let binding_service = AgentBindingService::new(self.pool.clone());
        let binding = binding_service
            .binding_for_session(session_id)
            .await?
            .ok_or(TurnTimelineServiceError::CapabilityUnavailable)?;
        let source_pending = !binding.discovered
            && all_turns.len() == 1
            && ranges.len() == 1
            && ranges[0].tail_cursor.is_none();
        let backend = agent_clients::turn_timeline_backend_for(&binding.client_type)
            .ok_or(TurnTimelineServiceError::CapabilityUnavailable)?;
        let source = match backend.resolver.resolve(&AgentBindingResolveRequest {
            id: binding.id.clone(),
            session_id: binding.session_id,
            client_type: binding.client_type,
            launch_cwd: binding.launch_cwd.into(),
            client_session_key: binding.client_session_key,
        }) {
            Ok(source) => source,
            Err(error) => {
                let error = classify_adapter_error(error);
                if source_pending && matches!(error, TurnTimelineServiceError::SourceUnavailable) {
                    return Ok(Vec::new());
                }
                return Err(error);
            }
        };
        let items = match backend
            .reader
            .read_turn_ranges(TurnTimelineReadRequest { source, ranges })
        {
            Ok(items) => items,
            Err(error) => {
                let error = classify_reader_error(error);
                if source_pending && matches!(error, TurnTimelineServiceError::SourceUnavailable) {
                    return Ok(Vec::new());
                }
                return Err(error);
            }
        };
        if !binding.discovered {
            binding_service.mark_discovered(&binding.id).await?;
        }
        Ok(items)
    }
}

fn turns_by_id(turns: &[TurnRow]) -> HashMap<&str, &TurnRow> {
    turns
        .iter()
        .map(|turn| (turn.turn_id.as_str(), turn))
        .collect()
}

fn topology_parent<'a>(
    turn: &'a TurnRow,
    by_id: &HashMap<&str, &'a TurnRow>,
) -> Result<Option<&'a str>, TurnTimelineServiceError> {
    match (
        turn.topology_status.as_str(),
        turn.parent_turn_id.as_deref(),
    ) {
        ("unknown", None) => Err(TurnTimelineServiceError::TopologyUnknown {
            turn_id: turn.turn_id.clone(),
        }),
        ("root", None) => Ok(None),
        ("linked", Some(parent_id))
            if by_id
                .get(parent_id)
                .is_some_and(|parent| parent.turn_id.as_str() < turn.turn_id.as_str()) =>
        {
            Ok(Some(parent_id))
        }
        _ => Err(TurnTimelineServiceError::TopologyInvalid {
            turn_id: turn.turn_id.clone(),
        }),
    }
}

fn ancestor_chain<'a>(
    leaf_turn_id: &str,
    by_id: &HashMap<&str, &'a TurnRow>,
) -> Result<Vec<&'a TurnRow>, TurnTimelineServiceError> {
    let mut chain = Vec::new();
    let mut current_id = leaf_turn_id;
    let mut visited = HashSet::new();
    loop {
        if !visited.insert(current_id) {
            return Err(TurnTimelineServiceError::TopologyInvalid {
                turn_id: current_id.to_string(),
            });
        }
        let turn = by_id
            .get(current_id)
            .copied()
            .ok_or(TurnTimelineServiceError::TurnNotFound)?;
        chain.push(turn);
        match topology_parent(turn, by_id)? {
            Some(parent_id) => current_id = parent_id,
            None => break,
        }
    }
    chain.reverse();
    Ok(chain)
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
