use std::collections::{HashMap, HashSet};

use serde::Deserialize;

use crate::topology::{
    TopologyDiagnostic, TopologyResolution, TopologyResolveRequest, TopologyResolveResult,
    TurnTopologyResolver,
};

use super::raw_transcripts::PiJsonlV2Cursor;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PiTopologyEvidence {
    entries: Vec<PiTopologyEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PiTopologyEntry {
    id: String,
    kind: PiTopologyEntryKind,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum PiTopologyEntryKind {
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

        let mut candidates_by_anchor: HashMap<String, (i64, String)> = HashMap::new();
        for candidate in request.earlier_turns {
            let Some(tail_cursor) = candidate.tail_cursor else {
                return unknown(TopologyDiagnostic::CandidateBoundaryMissing);
            };
            let Ok(cursor) = PiJsonlV2Cursor::decode(&tail_cursor, &request.binding_id) else {
                return unknown(TopologyDiagnostic::CursorInvalid);
            };
            let Some(anchor) = cursor.native_entry_anchor else {
                return unknown(TopologyDiagnostic::CandidateBoundaryMissing);
            };
            if candidates_by_anchor.contains_key(&anchor) {
                return unknown(TopologyDiagnostic::EvidenceInvalid);
            }
            candidates_by_anchor.insert(anchor, (candidate.turn_index, candidate.turn_id));
        }

        for entry in evidence.entries.iter().rev() {
            if let Some((_, parent_turn_id)) = candidates_by_anchor.get(&entry.id) {
                return TopologyResolveResult {
                    resolution: TopologyResolution::Linked {
                        parent_turn_id: parent_turn_id.clone(),
                    },
                    diagnostic: TopologyDiagnostic::ParentMatched,
                };
            }
            if entry.kind == PiTopologyEntryKind::UserMessage {
                return unknown(TopologyDiagnostic::ParentNotFound);
            }
        }

        unknown(TopologyDiagnostic::ParentNotFound)
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
