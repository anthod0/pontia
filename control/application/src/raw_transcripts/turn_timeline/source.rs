use std::collections::HashMap;

use pontia_agent_clients::{
    self as agent_clients, TimelineSourceBehavior,
    raw_transcripts::{
        AgentBindingResolveRequest, TurnTimelineItem, TurnTimelineRange, TurnTimelineReadError,
        TurnTimelineReadRequest,
    },
};
use pontia_core::{domain::TurnState, error::Error};
use pontia_storage_sqlite::{models::turns::TurnRow, repositories::turns::SqliteTurnRepository};

use super::{TurnTimelineGroup, TurnTimelineService, TurnTimelineServiceError};
use crate::{AgentBindingService, ExternalQueryService};

impl TurnTimelineService {
    pub(super) async fn read_selected_groups(
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

    pub(super) async fn read_selected_turns(
        &self,
        session_id: &str,
        all_turns: &[TurnRow],
        selected: &[&TurnRow],
    ) -> Result<Vec<TurnTimelineItem>, TurnTimelineServiceError> {
        if selected.is_empty() {
            return Ok(Vec::new());
        }

        let session = ExternalQueryService::new(self.pool.clone())
            .get_session(session_id)
            .await?
            .ok_or(TurnTimelineServiceError::SessionNotFound)?;
        let timeline_source = agent_clients::get_client_spec(&session.client_type)
            .map(|spec| spec.adapter.timeline_source)
            .unwrap_or(TimelineSourceBehavior::Unsupported);
        match timeline_source {
            TimelineSourceBehavior::Unsupported => {
                return Err(TurnTimelineServiceError::CapabilityUnavailable);
            }
            TimelineSourceBehavior::Transcript => {}
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
            client_session_file: binding.client_session_file.map(Into::into),
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
