use pontia_agent_clients::pi::raw_transcripts::{PiJsonlV2Cursor, TimelineBoundaryRelation};
use pontia_agent_clients::{
    TopologyDiagnostic, TopologyResolution, TopologyResolveRequest, TurnTopologyCandidate,
    topology_backend_for,
};
use serde_json::json;

fn cursor(binding_id: &str, anchor: &str) -> String {
    PiJsonlV2Cursor {
        binding_id: binding_id.to_string(),
        byte_offset: 42,
        native_entry_anchor: Some(anchor.to_string()),
        relation: TimelineBoundaryRelation::After,
    }
    .encode()
}

fn candidate(turn_id: &str, turn_index: i64, anchor: Option<&str>) -> TurnTopologyCandidate {
    TurnTopologyCandidate {
        turn_id: turn_id.to_string(),
        turn_index,
        tail_cursor: anchor.map(|anchor| cursor("binding_1", anchor)),
    }
}

fn resolve(
    evidence: Option<serde_json::Value>,
    earlier_turns: Vec<TurnTopologyCandidate>,
) -> pontia_agent_clients::TopologyResolveResult {
    topology_backend_for("pi")
        .expect("pi topology backend")
        .resolver
        .resolve(TopologyResolveRequest {
            binding_id: "binding_1".to_string(),
            current_turn_id: "turn_current".to_string(),
            current_turn_index: 4,
            earlier_turns,
            evidence,
        })
}

#[test]
fn empty_or_configuration_only_pi_context_is_root() {
    for entries in [
        json!([]),
        json!([
            {"id": "model", "kind": "model_change"},
            {"id": "thinking", "kind": "thinking_level_change"},
            {"id": "label", "kind": "label"},
            {"id": "info", "kind": "session_info"}
        ]),
    ] {
        let result = resolve(Some(json!({"entries": entries})), vec![]);
        assert_eq!(result.resolution, TopologyResolution::Root);
        assert_eq!(result.diagnostic, TopologyDiagnostic::RootContext);
    }
}

#[test]
fn pi_context_links_to_nearest_earlier_tail_across_intermediate_entries() {
    let result = resolve(
        Some(json!({"entries": [
            {"id": "user_1", "kind": "user_message"},
            {"id": "assistant_1", "kind": "assistant_message"},
            {"id": "branch", "kind": "branch_summary"},
            {"id": "compact", "kind": "compaction"},
            {"id": "model", "kind": "model_change"},
            {"id": "thinking", "kind": "thinking_level_change"},
            {"id": "label", "kind": "label"},
            {"id": "session", "kind": "session_info"},
            {"id": "custom", "kind": "custom"},
            {"id": "custom_message", "kind": "custom_message"},
            {"id": "other_message", "kind": "other_message"},
            {"id": "other", "kind": "other"},
            {"id": "tool", "kind": "tool_result_message"}
        ]})),
        vec![
            candidate("turn_1", 1, Some("assistant_1")),
            candidate("turn_2", 2, Some("missing_from_path")),
        ],
    );

    assert_eq!(
        result.resolution,
        TopologyResolution::Linked {
            parent_turn_id: "turn_1".to_string(),
        }
    );
    assert_eq!(result.diagnostic, TopologyDiagnostic::ParentMatched);
}

#[test]
fn uncorrelatable_conversation_context_remains_unknown() {
    let result = resolve(
        Some(json!({"entries": [
            {"id": "unknown_user", "kind": "user_message"}
        ]})),
        vec![candidate("turn_1", 1, Some("assistant_1"))],
    );

    assert_eq!(result.resolution, TopologyResolution::Unknown);
    assert_eq!(result.diagnostic, TopologyDiagnostic::ParentNotFound);
}

#[test]
fn uncorrelatable_user_entry_blocks_fallback_to_an_older_turn() {
    let result = resolve(
        Some(json!({"entries": [
            {"id": "user_1", "kind": "user_message"},
            {"id": "assistant_1", "kind": "assistant_message"},
            {"id": "untracked_user", "kind": "user_message"},
            {"id": "untracked_assistant", "kind": "assistant_message"}
        ]})),
        vec![candidate("turn_1", 1, Some("assistant_1"))],
    );

    assert_eq!(result.resolution, TopologyResolution::Unknown);
    assert_eq!(result.diagnostic, TopologyDiagnostic::ParentNotFound);
}

#[test]
fn malformed_evidence_and_invalid_cursor_scope_are_safe_unknowns() {
    let cases = [
        (
            Some(json!({"entries": [
                {"id": "duplicate", "kind": "user_message"},
                {"id": "duplicate", "kind": "assistant_message"}
            ]})),
            vec![],
            TopologyDiagnostic::EvidenceInvalid,
        ),
        (
            Some(json!({"entries": [{"id": "assistant_1", "kind": "assistant_message"}]})),
            vec![TurnTopologyCandidate {
                turn_id: "turn_1".to_string(),
                turn_index: 1,
                tail_cursor: Some(cursor("other_binding", "assistant_1")),
            }],
            TopologyDiagnostic::CursorInvalid,
        ),
        (None, vec![], TopologyDiagnostic::EvidenceMissing),
        (
            Some(json!({"entries": [
                {"id": "assistant_1", "kind": "assistant_message"}
            ]})),
            vec![
                candidate("turn_1", 1, Some("assistant_1")),
                candidate("turn_2", 2, None),
            ],
            TopologyDiagnostic::CandidateBoundaryMissing,
        ),
    ];

    for (evidence, candidates, diagnostic) in cases {
        let result = resolve(evidence, candidates);
        assert_eq!(result.resolution, TopologyResolution::Unknown);
        assert_eq!(result.diagnostic, diagnostic);
    }
}

#[test]
fn duplicate_candidate_entry_identity_is_an_ambiguous_unknown() {
    let result = resolve(
        Some(json!({"entries": [
            {"id": "shared_assistant", "kind": "assistant_message"}
        ]})),
        vec![
            candidate("turn_1", 1, Some("shared_assistant")),
            candidate("turn_2", 2, Some("shared_assistant")),
        ],
    );

    assert_eq!(result.resolution, TopologyResolution::Unknown);
    assert_eq!(result.diagnostic, TopologyDiagnostic::EvidenceInvalid);
}

#[test]
fn restored_pi_context_creates_siblings_without_changing_older_descendants() {
    let restored = resolve(
        Some(json!({"entries": [
            {"id": "user_1", "kind": "user_message"},
            {"id": "assistant_1", "kind": "assistant_message"}
        ]})),
        vec![
            candidate("turn_1", 1, Some("assistant_1")),
            candidate("turn_2", 2, Some("assistant_2")),
            candidate("turn_3", 3, Some("assistant_3")),
        ],
    );
    assert_eq!(
        restored.resolution,
        TopologyResolution::Linked {
            parent_turn_id: "turn_1".to_string(),
        }
    );

    let continuation = resolve(
        Some(json!({"entries": [
            {"id": "user_1", "kind": "user_message"},
            {"id": "assistant_1", "kind": "assistant_message"},
            {"id": "user_4", "kind": "user_message"},
            {"id": "assistant_4", "kind": "assistant_message"}
        ]})),
        vec![
            candidate("turn_1", 1, Some("assistant_1")),
            candidate("turn_2", 2, Some("assistant_2")),
            candidate("turn_3", 3, Some("assistant_3")),
            candidate("turn_4", 4, Some("assistant_4")),
        ],
    );
    assert_eq!(
        continuation.resolution,
        TopologyResolution::Linked {
            parent_turn_id: "turn_4".to_string(),
        }
    );
}
