use std::collections::{HashMap, HashSet};

use pontia_storage_sqlite::models::turns::TurnRow;

use super::TurnTimelineServiceError;

pub(super) fn turns_by_id(turns: &[TurnRow]) -> HashMap<&str, &TurnRow> {
    turns
        .iter()
        .map(|turn| (turn.turn_id.as_str(), turn))
        .collect()
}

pub(super) fn topology_parent<'a>(
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

pub(super) fn ancestor_chain<'a>(
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
