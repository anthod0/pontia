use std::collections::HashMap;

use crate::client_contract::raw_transcripts::{
    AgentBindingResolveRequest, TurnTimelineItem, TurnTimelineRange, TurnTimelineReadError,
    TurnTimelineReadRequest,
};
use pontia_core::{domain::TurnState, error::Error};
use pontia_storage_sqlite::{models::turns::TurnRow, repositories::turns::SqliteTurnRepository};

use super::{TurnHistoryIssue, TurnTimelineGroup, TurnTimelineService, TurnTimelineServiceError};
use crate::{AgentBindingService, ExternalQueryService};

impl TurnTimelineService {
    pub(super) async fn read_selected_groups(
        &self,
        session_id: &str,
        all_turns: &[TurnRow],
        selected: &[&TurnRow],
    ) -> Result<Vec<TurnTimelineGroup>, TurnTimelineServiceError> {
        let mut readable = selected.to_vec();
        let mut issues = HashMap::new();
        let items = loop {
            match self
                .read_selected_turns(session_id, all_turns, &readable)
                .await
            {
                Ok(items) => break items,
                Err(error) => {
                    let (turn_id, issue) = match &error {
                        TurnTimelineServiceError::TurnUnavailable { turn_id }
                        | TurnTimelineServiceError::NativeAssociationUnavailable { turn_id } => {
                            (turn_id, TurnHistoryIssue::RangeUnavailable)
                        }
                        TurnTimelineServiceError::TimelineInvalid { turn_id } => {
                            (turn_id, TurnHistoryIssue::RangeInvalid)
                        }
                        _ => return Err(error),
                    };
                    let Some(index) = readable.iter().position(|turn| turn.turn_id == *turn_id)
                    else {
                        return Err(error);
                    };
                    issues.insert(turn_id.clone(), issue);
                    readable.remove(index);
                }
            }
        };
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
                history_issue: issues.remove(&turn.turn_id).or_else(|| {
                    (turn.topology_status == "unknown").then_some(TurnHistoryIssue::TopologyUnknown)
                }),
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
            .with_clients(self.clients.clone())
            .get_session(session_id)
            .await?
            .ok_or(TurnTimelineServiceError::SessionNotFound)?;
        let active_turn_id = SqliteTurnRepository::new(self.pool.clone())
            .active_turn(session_id)
            .await?
            .map(|turn| turn.turn_id);
        let newest_turn_id = all_turns.last().map(|turn| turn.turn_id.as_str());
        if !self
            .clients
            .spec(&session.client_type)
            .is_some_and(|spec| spec.adapter.native_turn_identity)
        {
            if !session.capabilities.timeline {
                return Err(TurnTimelineServiceError::CapabilityUnavailable);
            }
            for turn in selected {
                let active = active_turn_id.as_deref() == Some(turn.turn_id.as_str())
                    && newest_turn_id == Some(turn.turn_id.as_str())
                    && turn.state.parse::<TurnState>()?.is_active();
                if turn.head_cursor.is_none() || (!active && turn.tail_cursor.is_none()) {
                    return Err(TurnTimelineServiceError::TurnUnavailable {
                        turn_id: turn.turn_id.clone(),
                    });
                }
            }
        }
        let binding_service = AgentBindingService::new(self.pool.clone());
        let binding = binding_service
            .binding_for_session(session_id)
            .await?
            .ok_or(TurnTimelineServiceError::CapabilityUnavailable)?;
        let source_pending = !self
            .clients
            .spec(&binding.client_type)
            .is_some_and(|spec| spec.adapter.native_turn_identity)
            && !binding.discovered
            && all_turns.len() == 1
            && selected.len() == 1
            && selected[0].tail_cursor.is_none()
            && active_turn_id.as_deref() == Some(selected[0].turn_id.as_str());
        let backend = self
            .clients
            .timeline(&binding.client_type)
            .ok_or(TurnTimelineServiceError::CapabilityUnavailable)?;
        let source = match backend.resolver.resolve(&AgentBindingResolveRequest {
            id: binding.id.clone(),
            session_id: binding.session_id.clone(),
            client_type: binding.client_type.clone(),
            client_session_key: binding.client_session_key.clone(),
            client_session_file: binding.client_session_file.clone().map(Into::into),
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
        if !session.capabilities.timeline {
            return Err(TurnTimelineServiceError::CapabilityUnavailable);
        }
        let mut ranges = Vec::with_capacity(selected.len());
        for turn in selected {
            let active = active_turn_id.as_deref() == Some(turn.turn_id.as_str())
                && newest_turn_id == Some(turn.turn_id.as_str())
                && turn.state.parse::<TurnState>()?.is_active();
            let is_first = all_turns.first().map(|first| first.turn_id.as_str())
                == Some(turn.turn_id.as_str());
            let recovered;
            let turn = if turn.head_cursor.is_none() || (!active && turn.tail_cursor.is_none()) {
                recovered = self
                    .recover_range(&binding, &source, turn, active, is_first)
                    .await?;
                &recovered
            } else {
                turn
            };
            ranges.push(TurnTimelineRange {
                turn_id: turn.turn_id.clone(),
                is_first_session_turn: is_first,
                head_cursor: turn.head_cursor.clone().ok_or_else(|| {
                    TurnTimelineServiceError::TurnUnavailable {
                        turn_id: turn.turn_id.clone(),
                    }
                })?,
                tail_cursor: turn.tail_cursor.clone(),
            });
        }
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

pub(super) fn classify_adapter_error(error: Error) -> TurnTimelineServiceError {
    let message = error.to_string();
    if message.contains("source_unavailable:") {
        return TurnTimelineServiceError::SourceUnavailable;
    }
    match error {
        Error::Conflict {
            code: "timeline_pending",
            ..
        } => TurnTimelineServiceError::Pending,
        Error::Conflict {
            code: "timeline_source_identity_mismatch",
            ..
        } => TurnTimelineServiceError::SourceIdentityMismatch,
        Error::CapabilityUnavailable(_) => TurnTimelineServiceError::CapabilityUnavailable,
        error => TurnTimelineServiceError::Inner(error),
    }
}

pub(super) fn classify_reader_error(error: TurnTimelineReadError) -> TurnTimelineServiceError {
    match error {
        TurnTimelineReadError::InvalidRange { turn_id, .. } => {
            TurnTimelineServiceError::TimelineInvalid { turn_id }
        }
        TurnTimelineReadError::Inner(error) => classify_adapter_error(error),
    }
}
