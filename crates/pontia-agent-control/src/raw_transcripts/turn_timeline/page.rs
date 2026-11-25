use pontia_storage_sqlite::repositories::turns::SqliteTurnRepository;

use super::{
    TurnTimelineDirection, TurnTimelinePage, TurnTimelineService, TurnTimelineServiceError,
};
use crate::ControlQueryService;

impl TurnTimelineService {
    pub async fn page(
        &self,
        session_id: String,
        direction: TurnTimelineDirection,
        anchor_turn_id: Option<String>,
        limit: usize,
    ) -> Result<TurnTimelinePage, TurnTimelineServiceError> {
        let mut scoped = self.request_scope();
        let service = &mut scoped;
        let session = ControlQueryService::new(service.pool.clone())
            .get_session(&session_id)
            .await?
            .ok_or(TurnTimelineServiceError::SessionNotFound)?;

        service.try_recover_history(&session_id).await;
        let mut turns = SqliteTurnRepository::new(service.pool.clone())
            .list_turns(&session_id)
            .await?;
        let mut history_source = None;
        let binding = crate::AgentBindingService::new(service.pool.clone())
            .binding_for_session(&session_id)
            .await?;
        if binding.is_none()
            && service
                .clients
                .data(&session.client_type)
                .is_some_and(|data| data.timeline_uses_lifecycle_anchors())
            && matches!(session.state.as_str(), "starting" | "created")
        {
            return Err(TurnTimelineServiceError::Pending);
        }
        if service.require_source && binding.is_none() {
            return Err(TurnTimelineServiceError::SourceUnavailable);
        }
        if let Some(binding) = binding
            && let Some(history) = service.native_history(&binding).await
            && let Some(selection) = history
                .select_timeline(
                    turns
                        .iter()
                        .map(
                            |turn| crate::client_contract::raw_transcripts::SessionTimelineTurn {
                                turn_id: turn.turn_id.clone(),
                                head_cursor: turn.head_cursor.clone(),
                                tail_cursor: turn.tail_cursor.clone(),
                            },
                        )
                        .collect(),
                    service.history_source,
                )
                .await
                .map_err(super::source::classify_adapter_error)?
        {
            history_source = Some(selection.source);
            let mut by_id: std::collections::HashMap<_, _> = turns
                .into_iter()
                .map(|turn| (turn.turn_id.clone(), turn))
                .collect();
            turns = selection
                .turn_ids
                .into_iter()
                .filter_map(|id| by_id.remove(&id))
                .collect();
        }
        service.history_source = history_source;
        if turns.is_empty() {
            if anchor_turn_id.is_some() {
                return Err(TurnTimelineServiceError::TurnNotFound);
            }
            let items = service.read_session_page(&session_id, &turns, &[]).await?;
            return Ok(TurnTimelinePage {
                session_id,
                direction,
                items,
                next_turn_id: None,
                history_source,
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
        if direction == TurnTimelineDirection::Backward {
            selected.reverse();
        }
        let items = service
            .read_session_page(&session_id, &turns, &selected)
            .await?;

        Ok(TurnTimelinePage {
            session_id,
            direction,
            items,
            next_turn_id,
            history_source,
        })
    }
}
