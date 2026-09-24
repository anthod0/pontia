use std::collections::{HashMap, HashSet};

use pontia_application::client_contract::{
    TopologyDiagnostic, TopologyResolution, TopologyResolveRequest, TopologyResolveResult,
    TurnTopologyCandidate, TurnTopologyResolver,
    history::{HistoryRecoveryRequest, RecoveredTurnHistory, TurnHistoryRecoverer},
};
use pontia_core::{
    Error, Result,
    domain::{TurnState, TurnTopology},
};

use super::{
    PiJsonlV2Cursor, PiTimelineAdapter, TimelineBoundaryRelation,
    source::{read_range_from_source, source_len},
    timeline::{ParsedEntry, parse_window},
};
use crate::topology::{
    PiTopologyEntry, PiTopologyEntryKind, PiTopologyEvidence, PiTopologyResolver,
};

impl TurnHistoryRecoverer for PiTimelineAdapter {
    fn recover(&self, request: HistoryRecoveryRequest) -> Result<Vec<RecoveredTurnHistory>> {
        let source = &request.source;
        if source.client_type != "pi" || source.format != "pi-jsonl" {
            return Err(Error::CapabilityUnavailable(
                "unsupported Pi history source".into(),
            ));
        }
        let bytes = read_range_from_source(source, 0, source_len(source)?)?;
        // A live writer may be appending the next record. Only sealed, complete records
        // can support recovery of older Turns.
        let complete_end = bytes
            .iter()
            .rposition(|byte| *byte == b'\n')
            .map_or(0, |i| i + 1);
        let (entries, count) = parse_window("history", &bytes[..complete_end], 0)
            .map_err(|_| Error::Domain("Pi history recovery: invalid native JSONL".into()))?;
        let by_id: HashMap<_, _> = entries
            .iter()
            .map(|(id, entry)| (id.as_str(), entry))
            .collect();
        if entries.len() != count || by_id.len() != entries.len() {
            return Err(Error::Domain(
                "Pi history recovery: missing or duplicate native identity".into(),
            ));
        }
        let heads: Vec<_> = request
            .turns
            .iter()
            .enumerate()
            .map(|(index, turn)| {
                let head =
                    PiJsonlV2Cursor::decode(turn.head_cursor.as_deref()?, &source.id).ok()?;
                if head.byte_offset > complete_end
                    || (head.native_entry_anchor.is_none() && index != 0)
                {
                    return None;
                }
                if head.native_entry_anchor.is_none()
                    && entries.iter().any(|(_, entry)| {
                        entry.end <= head.byte_offset && entry.value["type"] != "session"
                    })
                {
                    return None;
                }
                if let Some(anchor) = head.native_entry_anchor.as_deref() {
                    let entry = by_id.get(anchor)?;
                    if head.byte_offset != 0 && entry.end > head.byte_offset {
                        return None;
                    }
                }
                Some(head)
            })
            .collect();
        let mut turns = request.turns;
        let mut recovered = Vec::new();
        for index in 0..turns.len() {
            let turn = &turns[index];
            let needs_tail = turn.state == TurnState::Abandoned && turn.tail_cursor.is_none();
            if !needs_tail && turn.topology != TurnTopology::Unknown {
                continue;
            }
            let tail_cursor = if needs_tail {
                heads[index]
                    .as_ref()
                    .and_then(|head| recover_tail(head, &heads[index + 1..], &entries, &by_id))
            } else {
                None
            };
            let topology = if turn.topology == TurnTopology::Unknown {
                heads[index]
                    .as_ref()
                    .and_then(|head| native_context(head, &by_id))
                    .map(|evidence| {
                        PiTopologyResolver::new().resolve(TopologyResolveRequest {
                            binding_id: source.id.clone(),
                            current_turn_id: turn.turn_id.clone(),
                            earlier_turns: turns[..index]
                                .iter()
                                .map(|candidate| TurnTopologyCandidate {
                                    turn_id: candidate.turn_id.clone(),
                                    tail_cursor: candidate.tail_cursor.clone(),
                                })
                                .collect(),
                            evidence: Some(
                                serde_json::to_value(evidence)
                                    .expect("serializable native context"),
                            ),
                        })
                    })
                    .unwrap_or(TopologyResolveResult {
                        resolution: TopologyResolution::Unknown,
                        diagnostic: TopologyDiagnostic::EvidenceInvalid,
                    })
            } else {
                TopologyResolveResult {
                    resolution: TopologyResolution::Unknown,
                    diagnostic: TopologyDiagnostic::CandidateBoundaryMissing,
                }
            };
            recovered.push(RecoveredTurnHistory {
                turn_id: turn.turn_id.clone(),
                tail_cursor: tail_cursor.clone(),
                topology,
            });
            if let Some(tail) = tail_cursor {
                turns[index].tail_cursor = Some(tail);
            }
        }
        Ok(recovered)
    }
}

fn recover_tail(
    head: &PiJsonlV2Cursor,
    later_heads: &[Option<PiJsonlV2Cursor>],
    entries: &[(String, ParsedEntry)],
    by_id: &HashMap<&str, &ParsedEntry>,
) -> Option<String> {
    // A later captured head seals the physical read window, including when the
    // user resumed a different branch. Offsets never determine the semantic parent.
    let end = later_heads.first()?.as_ref()?.byte_offset;
    if end <= head.byte_offset {
        return None;
    }
    let start = if head.byte_offset == 0 {
        head.native_entry_anchor
            .as_deref()
            .and_then(|id| by_id.get(id))
            .map_or(0, |entry| entry.end)
    } else {
        head.byte_offset
    };
    let window: Vec<_> = entries
        .iter()
        .filter(|(_, entry)| entry.start >= start && entry.end <= end)
        .collect();
    let (last_id, last) = *window.last()?;
    if window.first()?.1.start != start || last.end != end {
        return None;
    }
    let mut parent = head.native_entry_anchor.as_deref();
    let mut users = 0;
    for (index, (id, entry)) in window.iter().enumerate() {
        // The session header identifies the file; it is not a node in Pi's branch.
        if index == 0 && entry.start == 0 && entry.value["type"] == "session" {
            continue;
        }
        if entry.parent_id.as_deref() != parent {
            return None;
        }
        users += usize::from(entry_kind(entry) == PiTopologyEntryKind::UserMessage);
        parent = Some(id);
    }
    if users != 1 {
        return None;
    }
    Some(
        PiJsonlV2Cursor {
            binding_id: head.binding_id.clone(),
            byte_offset: end,
            native_entry_anchor: Some(last_id.clone()),
            relation: TimelineBoundaryRelation::After,
        }
        .encode(),
    )
}

fn native_context(
    head: &PiJsonlV2Cursor,
    by_id: &HashMap<&str, &ParsedEntry>,
) -> Option<PiTopologyEvidence> {
    let mut entries = Vec::new();
    let mut current = head.native_entry_anchor.as_deref();
    let mut visited = HashSet::new();
    while let Some(id) = current {
        if !visited.insert(id) {
            return None;
        }
        let entry = by_id.get(id)?;
        entries.push(PiTopologyEntry {
            id: id.to_string(),
            kind: entry_kind(entry),
        });
        current = entry.parent_id.as_deref();
        if let Some(parent) = current
            && by_id.get(parent)?.end > entry.start
        {
            return None;
        }
    }
    entries.reverse();
    Some(PiTopologyEvidence { entries })
}

fn entry_kind(entry: &ParsedEntry) -> PiTopologyEntryKind {
    use PiTopologyEntryKind::*;
    match entry.value["type"].as_str() {
        Some("message") => match entry.value["message"]["role"].as_str() {
            Some("user") => UserMessage,
            Some("assistant") => AssistantMessage,
            Some("toolResult") => ToolResultMessage,
            _ => OtherMessage,
        },
        Some("thinking_level_change") => ThinkingLevelChange,
        Some("model_change") => ModelChange,
        Some("compaction") => Compaction,
        Some("branch_summary") => BranchSummary,
        Some("custom") => Custom,
        Some("custom_message") => CustomMessage,
        Some("label") => Label,
        Some("session_info") => SessionInfo,
        _ => Other,
    }
}
