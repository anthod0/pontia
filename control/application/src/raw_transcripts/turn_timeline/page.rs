use pontia_storage_sqlite::repositories::turns::SqliteTurnRepository;

use super::{
    TurnTimelineDirection, TurnTimelinePage, TurnTimelineService, TurnTimelineServiceError,
};
use crate::ExternalQueryService;

impl TurnTimelineService {
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
}
