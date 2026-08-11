use std::path::PathBuf;

use pontia_agent_clients::{
    pi::raw_transcripts::{PiAgentBindingResolver, PiJsonlV2Cursor, TimelineBoundaryRelation},
    raw_transcripts::{AgentBindingResolveRequest, AgentBindingResolver},
};
use serde::Serialize;
use serde_json::Value;

use super::{
    candidate_loading::CandidateRow,
    timeline_validation::{locate_entry_line_end, required, validate_offset},
};

#[derive(Debug, Serialize)]
pub(super) struct RepairPlan {
    pub(super) mode: &'static str,
    pub(super) database: String,
    pub(super) backup: Option<String>,
    pub(super) summary: RepairSummary,
    pub(super) candidates: Vec<RepairCandidate>,
}

#[derive(Debug, Serialize)]
pub(super) struct RepairSummary {
    pub(super) candidates: usize,
    pub(super) repairable: usize,
    pub(super) blocked: usize,
    pub(super) applied: usize,
}

#[derive(Debug, Serialize)]
pub(super) struct RepairCandidate {
    pub(super) event_id: String,
    pub(super) session_id: String,
    pub(super) turn_id: String,
    terminal_leaf_id: Option<String>,
    binding_id: Option<String>,
    source_file: Option<String>,
    pub(super) proposed_tail_cursor: Option<String>,
    pub(super) proposed_event_timeline_boundary: Option<Value>,
    status: &'static str,
    pub(super) errors: Vec<String>,
}

pub(super) fn build_candidate(
    row: CandidateRow,
    resolver: &PiAgentBindingResolver,
) -> RepairCandidate {
    let mut errors = Vec::new();
    let mut source_file = None;
    let mut proposed_tail_cursor = None;

    if row.turn_state != "interrupted" {
        errors.push(format!(
            "turn state is {}, expected interrupted",
            row.turn_state
        ));
    }
    if row.terminal_event_count != 1 {
        errors.push(format!(
            "turn has {} terminal lifecycle events, expected exactly one",
            row.terminal_event_count
        ));
    }
    if row.event_timeline_boundary.is_some() {
        errors.push("interrupted event already has timeline_boundary".to_string());
    }
    let terminal_leaf_id = required(&row.terminal_leaf_id, "terminal_leaf_id", &mut errors);
    let binding_id = required(&row.binding_id, "agent binding", &mut errors);
    let client_type = required(&row.binding_client_type, "binding client_type", &mut errors);
    let client_session_file = required(
        &row.client_session_file,
        "binding client_session_file",
        &mut errors,
    );
    if client_type.is_some_and(|value| value != "pi") {
        errors.push("agent binding client_type is not pi".to_string());
    }

    if let (
        Some(terminal_leaf_id),
        Some(binding_id),
        Some(client_type),
        Some(client_session_file),
    ) = (
        terminal_leaf_id,
        binding_id,
        client_type,
        client_session_file,
    ) {
        match resolver.resolve(&AgentBindingResolveRequest {
            id: binding_id.to_string(),
            session_id: row.session_id.clone(),
            client_type: client_type.to_string(),
            client_session_file: Some(PathBuf::from(client_session_file)),
        }) {
            Ok(source) => {
                source_file = Some(source.path.display().to_string());
                match locate_entry_line_end(&source.path, terminal_leaf_id) {
                    Ok(offset) => {
                        validate_offset(
                            offset,
                            binding_id,
                            row.head_cursor.as_deref(),
                            row.next_head_cursor.as_deref(),
                            &mut errors,
                        );
                        if errors.is_empty() {
                            proposed_tail_cursor = Some(
                                PiJsonlV2Cursor {
                                    binding_id: binding_id.to_string(),
                                    byte_offset: offset,
                                    native_entry_anchor: Some(terminal_leaf_id.to_string()),
                                    relation: TimelineBoundaryRelation::After,
                                }
                                .encode(),
                            );
                        }
                    }
                    Err(error) => errors.push(error),
                }
            }
            Err(error) => errors.push(format!("source resolution failed: {error}")),
        }
    }

    let proposed_event_timeline_boundary = proposed_tail_cursor
        .as_ref()
        .map(|cursor| serde_json::json!({ "position": "tail", "cursor": cursor }));
    RepairCandidate {
        event_id: row.event_id,
        session_id: row.session_id,
        turn_id: row.turn_id,
        terminal_leaf_id: row.terminal_leaf_id,
        binding_id: row.binding_id,
        source_file,
        proposed_tail_cursor,
        proposed_event_timeline_boundary,
        status: if errors.is_empty() {
            "repairable"
        } else {
            "blocked"
        },
        errors,
    }
}
