use std::collections::HashSet;

use pontia_storage_sqlite::repositories::turns::SqliteTurnRepository;

use super::{
    TurnTimelineService, TurnTimelineServiceError, TurnTreeHistoryPage,
    topology::{topology_parent, turns_by_id},
};
use crate::ExternalQueryService;

impl TurnTimelineService {
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
}
