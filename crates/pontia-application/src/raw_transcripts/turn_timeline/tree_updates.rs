use std::collections::HashMap;

use pontia_storage_sqlite::repositories::turns::SqliteTurnRepository;

use super::{
    TurnTimelineService, TurnTimelineServiceError, TurnTreeUpdatesPage,
    topology::{ancestor_chain, turns_by_id},
};
use crate::ExternalQueryService;

impl TurnTimelineService {
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
}
