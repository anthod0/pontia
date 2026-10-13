//! Pi branch ordering, Turn attribution, and page ownership of native checkpoints.
use super::{PiEntryCursor, invalid};
use crate::raw_transcripts::{PiJsonlV2Cursor, mapping::pi_entry_to_items};
use pontia_application::client_contract::raw_transcripts::{
    SessionTimelineSelection, TurnTimelineItem, TurnTimelineReadError,
};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

pub(crate) fn session_items(
    binding_id: &str,
    native_session: &str,
    entries: &[Value],
    leaf: Option<&str>,
    selection: SessionTimelineSelection,
) -> Result<Vec<TurnTimelineItem>, TurnTimelineReadError> {
    let mut by_id = HashMap::new();
    for (index, entry) in entries.iter().enumerate() {
        let id = entry["id"]
            .as_str()
            .ok_or_else(|| invalid("missing native entry id"))?;
        if by_id.insert(id, index).is_some() {
            return Err(invalid("duplicate native entry id").into());
        }
    }
    let chain = |head: Option<&str>, tail: Option<&str>| {
        let mut current = tail;
        let mut result = Vec::new();
        let mut visited = HashSet::new();
        while current != head {
            let id = current.ok_or_else(|| invalid("head is not an ancestor of tail"))?;
            if !visited.insert(id) {
                return Err(invalid("native branch contains a cycle"));
            }
            let index = *by_id
                .get(id)
                .ok_or_else(|| invalid("native branch anchor missing"))?;
            result.push(index);
            current = entries[index]["parentId"].as_str();
        }
        result.reverse();
        Ok(result)
    };
    let active: HashSet<usize> = chain(None, leaf)?.into_iter().collect();
    let decode = |cursor: &str| {
        if PiEntryCursor::is_entry(cursor) {
            PiEntryCursor::decode(cursor, binding_id, Some(native_session)).map(|c| c.anchor)
        } else {
            PiJsonlV2Cursor::decode(cursor, binding_id).map(|c| c.native_entry_anchor)
        }
    };
    let mut owners = HashMap::new();
    let mut starts = Vec::new();
    let selected: HashSet<&str> = selection
        .selected_turn_ids
        .iter()
        .map(String::as_str)
        .collect();
    for turn in &selection.turns {
        let Some(head_cursor) = &turn.head_cursor else {
            continue;
        };
        let error = |error: pontia_core::Error| TurnTimelineReadError::InvalidRange {
            turn_id: turn.turn_id.clone(),
            message: error.to_string(),
        };
        let requested = selected.contains(turn.turn_id.as_str());
        let head = match decode(head_cursor) {
            Ok(head) => head,
            Err(failure) if requested => return Err(error(failure)),
            Err(_) => continue,
        };
        if head.as_deref().is_some_and(|id| !by_id.contains_key(id)) {
            if requested {
                return Err(error(invalid("Turn head anchor missing")));
            }
            continue;
        }
        let start = head
            .as_deref()
            .and_then(|id| by_id.get(id))
            .map_or(0, |index| index + 1);
        starts.push((start, turn.turn_id.as_str()));
        let tail = match turn.tail_cursor.as_deref().map(decode).transpose() {
            Ok(tail) => tail,
            Err(failure) if requested => return Err(error(failure)),
            Err(_) => continue,
        };
        if tail.as_ref().is_some_and(Option::is_none) {
            if requested {
                return Err(error(invalid("Turn tail anchor missing")));
            }
            continue;
        }
        if !requested && tail.is_none() {
            continue;
        }
        let indices = match chain(
            head.as_deref(),
            tail.as_ref().map_or(leaf, |a| a.as_deref()),
        ) {
            Ok(indices) => indices,
            Err(failure) if requested => return Err(error(failure)),
            Err(_) => continue,
        };
        for index in indices {
            if owners.insert(index, turn.turn_id.as_str()).is_some() {
                return Err(invalid("overlapping Turn ranges").into());
            }
        }
    }
    starts.sort_by_key(|(index, _)| *index);
    let mut result = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        let owner = owners.get(&index).copied();
        for item in pi_entry_to_items(entry, 0) {
            if let Some(turn) = owner {
                if selected.contains(turn) {
                    result.push(TurnTimelineItem {
                        turn_id: Some(turn.into()),
                        item,
                    });
                }
            } else if item.kind == "context_compaction" && active.contains(&index) {
                let page = starts
                    .iter()
                    .find(|(start, _)| *start > index)
                    .or_else(|| starts.last());
                if selection.turns.is_empty()
                    || page.is_some_and(|(_, turn)| selected.contains(turn))
                {
                    result.push(TurnTimelineItem {
                        turn_id: None,
                        item,
                    });
                }
            }
        }
    }
    Ok(result)
}
