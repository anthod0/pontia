use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use pontia_application::client_contract::topology::{
    TopologyDiagnostic, TopologyResolution, TopologyResolveRequest, TopologyResolveResult,
    TurnTopologyResolver,
};

use super::raw_transcripts::PiJsonlV2Cursor;

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PiTopologyEvidence {
    pub(crate) entries: Vec<PiTopologyEntry>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PiTopologyEntry {
    pub(crate) id: String,
    pub(crate) kind: PiTopologyEntryKind,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PiTopologyEntryKind {
    UserMessage,
    AssistantMessage,
    ToolResultMessage,
    OtherMessage,
    ThinkingLevelChange,
    ModelChange,
    Compaction,
    BranchSummary,
    Custom,
    CustomMessage,
    Label,
    SessionInfo,
    Other,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct PiTopologyResolver;

impl PiTopologyResolver {
    pub const fn new() -> Self {
        Self
    }
}

impl TurnTopologyResolver for PiTopologyResolver {
    fn client_type(&self) -> &'static str {
        "pi"
    }

    fn resolve(&self, request: TopologyResolveRequest) -> TopologyResolveResult {
        let Some(evidence) = request.evidence else {
            return unknown(TopologyDiagnostic::EvidenceMissing);
        };
        let Ok(evidence) = serde_json::from_value::<PiTopologyEvidence>(evidence) else {
            return unknown(TopologyDiagnostic::EvidenceInvalid);
        };
        if !valid_entries(&evidence.entries) {
            return unknown(TopologyDiagnostic::EvidenceInvalid);
        }

        if evidence.entries.is_empty()
            || evidence
                .entries
                .iter()
                .all(|entry| entry.kind.is_configuration())
        {
            return TopologyResolveResult {
                resolution: TopologyResolution::Root,
                diagnostic: TopologyDiagnostic::RootContext,
            };
        }

        let mut candidates_by_anchor: HashMap<String, Option<String>> = HashMap::new();
        let mut unmatched = TopologyDiagnostic::ParentNotFound;
        for candidate in request.earlier_turns {
            let Some(tail_cursor) = candidate.tail_cursor else {
                unmatched = TopologyDiagnostic::CandidateBoundaryMissing;
                continue;
            };
            let anchor = if crate::history::PiEntryCursor::is_entry(&tail_cursor) {
                crate::history::PiEntryCursor::decode(&tail_cursor, &request.binding_id, None)
                    .map(|c| c.anchor)
            } else {
                PiJsonlV2Cursor::decode(&tail_cursor, &request.binding_id)
                    .map(|c| c.native_entry_anchor)
            };
            let Ok(anchor) = anchor else {
                unmatched = TopologyDiagnostic::CursorInvalid;
                continue;
            };
            let Some(anchor) = anchor else {
                unmatched = TopologyDiagnostic::CandidateBoundaryMissing;
                continue;
            };
            candidates_by_anchor
                .entry(anchor)
                .and_modify(|owner| *owner = None)
                .or_insert(Some(candidate.turn_id));
        }

        for entry in evidence.entries.iter().rev() {
            if let Some(owner) = candidates_by_anchor.get(&entry.id) {
                let Some(parent_turn_id) = owner else {
                    return unknown(TopologyDiagnostic::EvidenceInvalid);
                };
                return TopologyResolveResult {
                    resolution: TopologyResolution::Linked {
                        parent_turn_id: parent_turn_id.clone(),
                    },
                    diagnostic: TopologyDiagnostic::ParentMatched,
                };
            }
            if entry.kind == PiTopologyEntryKind::UserMessage {
                return unknown(unmatched);
            }
        }

        unknown(unmatched)
    }
}

fn unknown(diagnostic: TopologyDiagnostic) -> TopologyResolveResult {
    TopologyResolveResult {
        resolution: TopologyResolution::Unknown,
        diagnostic,
    }
}

fn valid_entries(entries: &[PiTopologyEntry]) -> bool {
    let mut ids = HashSet::new();
    entries
        .iter()
        .all(|entry| !entry.id.trim().is_empty() && ids.insert(entry.id.as_str()))
}

impl PiTopologyEntryKind {
    fn is_configuration(self) -> bool {
        matches!(
            self,
            Self::ThinkingLevelChange
                | Self::ModelChange
                | Self::Custom
                | Self::Label
                | Self::SessionInfo
        )
    }
}

pub(crate) fn entry_kind(entry: &serde_json::Value) -> PiTopologyEntryKind {
    use PiTopologyEntryKind::*;
    match entry["type"].as_str() {
        Some("message") => match entry["message"]["role"].as_str() {
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
