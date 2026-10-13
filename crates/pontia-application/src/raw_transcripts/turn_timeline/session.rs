use super::{
    TurnTimelineService, TurnTimelineServiceError,
    source::{classify_adapter_error, classify_reader_error},
};
use crate::client_contract::raw_transcripts::{
    SessionTimelineReadRequest, SessionTimelineSelection, SessionTimelineTurn, TurnTimelineItem,
};
use crate::{AgentBindingService, ExternalQueryService};
use pontia_storage_sqlite::models::turns::TurnRow;

impl TurnTimelineService {
    pub(super) async fn read_session_page(
        &self,
        session_id: &str,
        turns: &[TurnRow],
        selected: &[&TurnRow],
    ) -> Result<Vec<TurnTimelineItem>, TurnTimelineServiceError> {
        let session = ExternalQueryService::new(self.pool.clone())
            .with_clients(self.clients.clone())
            .get_session(session_id)
            .await?
            .ok_or(TurnTimelineServiceError::SessionNotFound)?;
        if !session.capabilities.compaction_history {
            return self.read_selected_turns(session_id, turns, selected).await;
        }
        self.selected_ranges(session_id, turns, selected).await?;
        let bindings = AgentBindingService::new(self.pool.clone());
        let Some(binding) = bindings.binding_for_session(session_id).await? else {
            return if turns.is_empty() {
                Ok(Vec::new())
            } else {
                Err(TurnTimelineServiceError::CapabilityUnavailable)
            };
        };
        let selection = SessionTimelineSelection {
            turns: turns
                .iter()
                .map(|turn| SessionTimelineTurn {
                    turn_id: turn.turn_id.clone(),
                    head_cursor: turn.head_cursor.clone(),
                    tail_cursor: turn.tail_cursor.clone(),
                })
                .collect(),
            selected_turn_ids: selected.iter().map(|turn| turn.turn_id.clone()).collect(),
        };
        let items = if let Some(history) = self.native_history(&binding).await {
            history
                .read_session_timeline(selection)
                .await
                .map_err(classify_reader_error)?
        } else {
            let backend = self
                .clients
                .timeline(&binding.client_type)
                .ok_or(TurnTimelineServiceError::CapabilityUnavailable)?;
            let source = backend
                .resolver
                .resolve(&super::binding_resolve_request(&binding))
                .map_err(classify_adapter_error)?;
            backend
                .reader
                .read_session_timeline(SessionTimelineReadRequest { source, selection })
                .map_err(classify_reader_error)?
        };
        for item in &items {
            if item.turn_id.is_none() && item.item.kind != "context_compaction" {
                return Err(pontia_core::Error::Domain(
                    "Only context_compaction checkpoints may have no Turn".into(),
                )
                .into());
            }
            if let Some(turn_id) = &item.turn_id
                && !selected.iter().any(|turn| &turn.turn_id == turn_id)
            {
                return Err(pontia_core::Error::Domain(
                    "Native timeline returned an unselected Turn".into(),
                )
                .into());
            }
        }
        if !binding.discovered {
            bindings.mark_discovered(&binding.id).await?;
        }
        Ok(items)
    }
}
