use super::{PiEntryCursor, PiHistory, RuntimeSnapshot, invalid};
use crate::{
    raw_transcripts::{
        PiAgentBindingResolver, PiJsonlV2Cursor, PiTimelineAdapter, mapping::pi_entry_to_items,
    },
    topology::{PiTopologyEntry, PiTopologyEvidence, PiTopologyResolver},
};
use pontia_application::client_contract::{
    TopologyDiagnostic, TopologyResolution, TopologyResolveRequest, TopologyResolveResult,
    TurnTopologyCandidate, TurnTopologyResolver,
    history::{
        HistoryRecoveryRequest, RecoveredTurnHistory, TurnHistoryCandidate, TurnHistoryRecoverer,
    },
    raw_transcripts::AgentBindingResolver,
};
use pontia_core::{
    Result,
    domain::{TurnState, TurnTopology},
};
use serde_json::Value;

impl PiHistory {
    pub(super) async fn recover_turns(
        &self,
        mut turns: Vec<TurnHistoryCandidate>,
    ) -> Result<Vec<RecoveredTurnHistory>> {
        let needed = |t: &TurnHistoryCandidate| {
            t.head_cursor.is_none()
                || t.topology == TurnTopology::Unknown
                || (t.state == TurnState::Abandoned && t.tail_cursor.is_none())
        };
        let generations = turns
            .iter()
            .map(|t| self.turn_generation(t.head_cursor.as_deref(), t.tail_cursor.as_deref()))
            .collect::<Result<Vec<_>>>()?;
        let legacy_needed = turns
            .iter()
            .zip(&generations)
            .any(|(t, entry)| needed(t) && !entry);
        let runtime_needed = turns
            .iter()
            .zip(&generations)
            .any(|(t, entry)| needed(t) && *entry);
        let mut result = Vec::new();
        let mut legacy_failure = None;
        if legacy_needed {
            match PiAgentBindingResolver::new()
                .resolve(&self.binding)
                .and_then(|source| {
                    PiTimelineAdapter::new().recover(HistoryRecoveryRequest {
                        source,
                        turns: turns.clone(),
                    })
                }) {
                Ok(recovered) => result.extend(recovered),
                Err(error) if runtime_needed => {
                    tracing::warn!(
                        diagnostic = "legacy_history_unavailable",
                        "legacy history recovery unavailable"
                    );
                    legacy_failure = Some(error);
                }
                Err(error) => return Err(error),
            }
            for recovered in &result {
                if let Some(tail) = &recovered.tail_cursor
                    && let Some(turn) = turns.iter_mut().find(|t| t.turn_id == recovered.turn_id)
                {
                    turn.tail_cursor = Some(tail.clone());
                }
            }
        }
        if !runtime_needed {
            return Ok(result);
        }
        let snapshot = match self.snapshot().await {
            Ok(snapshot) => snapshot,
            Err(_) if !result.is_empty() => {
                tracing::warn!(
                    diagnostic = "runtime_history_unavailable",
                    "runtime history recovery unavailable"
                );
                return Ok(result);
            }
            Err(error) => return Err(error),
        };
        for index in 0..turns.len() {
            let turn = &turns[index];
            if !needed(turn) || !generations[index] {
                continue;
            }
            let mut head_recovered = None;
            let head = match turn.head_cursor.as_deref() {
                Some(cursor) => PiEntryCursor::decode(
                    cursor,
                    &self.binding.id,
                    Some(&self.binding.client_session_key),
                )?,
                None => {
                    // Only unique native user evidence can recover a missing association.
                    let Some(input) = turn.input_summary.as_deref().filter(|s| !s.is_empty())
                    else {
                        continue;
                    };
                    let mut users = snapshot
                        .entries
                        .iter()
                        .filter(|e| user_input_matches(e, input));
                    let Some(entry) = users.next() else {
                        continue;
                    };
                    if users.next().is_some() {
                        continue;
                    }
                    let anchor = entry["parentId"].as_str().map(str::to_owned);
                    if anchor.is_none() && index != 0 {
                        continue;
                    }
                    let cursor = self.cursor(anchor);
                    head_recovered = Some(cursor.clone());
                    PiEntryCursor::decode(
                        &cursor,
                        &self.binding.id,
                        Some(&self.binding.client_session_key),
                    )?
                }
            };
            if head.anchor.is_none() && index != 0 {
                return Err(invalid("source origin is only valid for the first Turn"));
            }
            // Verify head membership before using it for recovery or topology.
            let context = snapshot.chain(None, head.anchor.as_deref())?;
            let tail = if turn.state == TurnState::Abandoned && turn.tail_cursor.is_none() {
                self.recover_tail(snapshot, &head, &turns[index + 1..])?
            } else {
                None
            };
            if let Some(tail) = turn.tail_cursor.as_deref().or(tail.as_deref()) {
                let terminal = PiEntryCursor::decode(
                    tail,
                    &self.binding.id,
                    Some(&self.binding.client_session_key),
                )?;
                snapshot.chain(head.anchor.as_deref(), terminal.anchor.as_deref())?;
            }
            let topology = if turn.topology == TurnTopology::Unknown {
                PiTopologyResolver::new().resolve(TopologyResolveRequest {
                    binding_id: self.binding.id.clone(),
                    current_turn_id: turn.turn_id.clone(),
                    earlier_turns: turns[..index]
                        .iter()
                        .map(|t| TurnTopologyCandidate {
                            turn_id: t.turn_id.clone(),
                            tail_cursor: t.tail_cursor.clone(),
                        })
                        .collect(),
                    evidence: Some(serde_json::to_value(PiTopologyEvidence {
                        entries: context
                            .iter()
                            .map(|e| PiTopologyEntry {
                                id: e["id"].as_str().unwrap().into(),
                                kind: crate::topology::entry_kind(e),
                            })
                            .collect(),
                    })?),
                })
            } else {
                TopologyResolveResult {
                    resolution: TopologyResolution::Unknown,
                    diagnostic: TopologyDiagnostic::CandidateBoundaryMissing,
                }
            };
            result.push(RecoveredTurnHistory {
                turn_id: turn.turn_id.clone(),
                head_cursor: head_recovered.clone(),
                tail_cursor: tail.clone(),
                topology,
            });
            if let Some(head) = head_recovered {
                turns[index].head_cursor = Some(head);
            }
            if let Some(tail) = tail {
                turns[index].tail_cursor = Some(tail);
            }
        }
        snapshot.connection.validate().await?;
        if result.is_empty()
            && let Some(error) = legacy_failure
        {
            return Err(error);
        }
        Ok(result)
    }
    fn recover_tail(
        &self,
        snapshot: &RuntimeSnapshot,
        head: &PiEntryCursor,
        later: &[TurnHistoryCandidate],
    ) -> Result<Option<String>> {
        let Some(next) = later.first() else {
            return Ok(None);
        };
        let Some(next_head) = &next.head_cursor else {
            return Ok(None);
        };
        let next_anchor = if PiEntryCursor::is_entry(next_head) {
            PiEntryCursor::decode(
                next_head,
                &self.binding.id,
                Some(&self.binding.client_session_key),
            )?
            .anchor
        } else {
            PiJsonlV2Cursor::decode(next_head, &self.binding.id)?.native_entry_anchor
        };
        // A later head on the same branch seals precisely that ancestor chain.
        if next_anchor != head.anchor
            && let Ok(entries) = snapshot.chain(head.anchor.as_deref(), next_anchor.as_deref())
            && entries.iter().filter(|e| user(e)).count() == 1
        {
            return Ok(next_anchor.map(|anchor| self.cursor(Some(anchor))));
        }
        // Branch switches can seal append evidence, but every entry in the old Turn
        // still has to form one continuous parent chain with exactly one user.
        let Some(next_input) = next.input_summary.as_deref() else {
            return Ok(None);
        };
        let mut users = snapshot.entries.iter().enumerate().filter(|(_, e)| {
            e["parentId"].as_str() == next_anchor.as_deref() && user_input_matches(e, next_input)
        });
        let Some((end, _)) = users.next() else {
            return Ok(None);
        };
        if users.next().is_some() {
            return Ok(None);
        }
        let start = head.anchor.as_ref().map_or(0, |id| snapshot.by_id[id] + 1);
        if end <= start {
            return Ok(None);
        }
        let window = &snapshot.entries[start..end];
        let mut parent = head.anchor.as_deref();
        for entry in window {
            if entry["parentId"].as_str() != parent {
                return Ok(None);
            }
            parent = entry["id"].as_str();
        }
        if window.iter().filter(|e| user(e)).count() != 1 {
            return Ok(None);
        }
        Ok(parent.map(|id| self.cursor(Some(id.into()))))
    }
}
fn user(entry: &Value) -> bool {
    entry["type"] == "message" && entry["message"]["role"] == "user"
}
fn user_input_matches(entry: &Value, summary: &str) -> bool {
    // events.ts persists at most 200 Unicode code points, rather than bytes.
    user(entry)
        && pi_entry_to_items(entry, 0)
            .first()
            .is_some_and(|item| item.content_preview.chars().take(200).eq(summary.chars()))
}
