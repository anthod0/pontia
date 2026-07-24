use std::fs;

use pontia_agent_clients::pi::raw_transcripts::{
    PiJsonlV2Cursor, PiTimelineAdapter, PiTurnUserEntryResolveError, PiTurnUserEntryResolveRequest,
    PiTurnUserEntryResolver, TimelineBoundaryRelation,
};
use pontia_agent_clients::raw_transcripts::ResolvedAgentBinding;
use tempfile::tempdir;

fn cursor(binding_id: &str, offset: usize, anchor: Option<&str>) -> String {
    PiJsonlV2Cursor {
        binding_id: binding_id.to_string(),
        byte_offset: offset,
        native_entry_anchor: anchor.map(ToString::to_string),
        relation: TimelineBoundaryRelation::After,
    }
    .encode()
}

#[test]
fn resolves_a_completed_root_turn_to_its_single_pi_user_entry() {
    let contents = concat!(
        "{\"type\":\"message\",\"id\":\"user-root\",\"parentId\":null,\"message\":{\"role\":\"user\",\"content\":\"hello\"}}\n",
        "{\"type\":\"message\",\"id\":\"answer-root\",\"parentId\":\"user-root\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"world\"}]}}\n"
    );
    let dir = tempdir().unwrap();
    let path = dir.path().join("session.jsonl");
    fs::write(&path, contents).unwrap();

    let resolved = PiTimelineAdapter::new()
        .resolve_user_entry(PiTurnUserEntryResolveRequest {
            source: ResolvedAgentBinding {
                id: "binding-1".to_string(),
                client_type: "pi".to_string(),
                format: "pi-jsonl".to_string(),
                path,
                fingerprint: None,
            },
            session_id: "session-1".to_string(),
            turn_session_id: "session-1".to_string(),
            turn_id: "turn-root".to_string(),
            is_first_session_turn: true,
            head_cursor: Some(cursor("binding-1", 0, None)),
            tail_cursor: Some(cursor("binding-1", contents.len(), Some("answer-root"))),
        })
        .unwrap();

    assert_eq!(resolved.entry_id, "user-root");
}

#[test]
fn distinguishes_a_stale_binding_from_a_malformed_cursor() {
    let contents = "{\"type\":\"message\",\"id\":\"user\",\"parentId\":null,\"message\":{\"role\":\"user\",\"content\":\"hello\"}}\n";
    let dir = tempdir().unwrap();
    let path = dir.path().join("session.jsonl");
    fs::write(&path, contents).unwrap();
    let source = ResolvedAgentBinding {
        id: "binding-current".to_string(),
        client_type: "pi".to_string(),
        format: "pi-jsonl".to_string(),
        path,
        fingerprint: None,
    };

    let stale = PiTimelineAdapter::new()
        .resolve_user_entry(PiTurnUserEntryResolveRequest {
            source: source.clone(),
            session_id: "session-1".to_string(),
            turn_session_id: "session-1".to_string(),
            turn_id: "turn-stale".to_string(),
            is_first_session_turn: true,
            head_cursor: Some(cursor("binding-old", 0, None)),
            tail_cursor: Some(cursor("binding-old", contents.len(), Some("user"))),
        })
        .unwrap_err();
    assert!(matches!(
        stale,
        PiTurnUserEntryResolveError::BindingStale { turn_id }
            if turn_id == "turn-stale"
    ));

    let malformed = PiTimelineAdapter::new()
        .resolve_user_entry(PiTurnUserEntryResolveRequest {
            source,
            session_id: "session-1".to_string(),
            turn_session_id: "session-1".to_string(),
            turn_id: "turn-malformed".to_string(),
            is_first_session_turn: true,
            head_cursor: Some("not-a-pi-cursor".to_string()),
            tail_cursor: Some(cursor("binding-current", contents.len(), Some("user"))),
        })
        .unwrap_err();
    assert!(matches!(
        malformed,
        PiTurnUserEntryResolveError::InvalidRange { turn_id }
            if turn_id == "turn-malformed"
    ));
}

#[test]
fn rejects_cross_session_targets_and_missing_boundaries_before_reading_pi_data() {
    let source = ResolvedAgentBinding {
        id: "binding-current".to_string(),
        client_type: "pi".to_string(),
        format: "pi-jsonl".to_string(),
        path: "/does/not/need/to/exist".into(),
        fingerprint: None,
    };
    let cross_session = PiTimelineAdapter::new()
        .resolve_user_entry(PiTurnUserEntryResolveRequest {
            source: source.clone(),
            session_id: "session-current".to_string(),
            turn_session_id: "session-other".to_string(),
            turn_id: "turn-other".to_string(),
            is_first_session_turn: false,
            head_cursor: None,
            tail_cursor: None,
        })
        .unwrap_err();
    assert!(matches!(
        cross_session,
        PiTurnUserEntryResolveError::SessionMismatch { turn_id }
            if turn_id == "turn-other"
    ));

    for (head_cursor, tail_cursor, expected_boundary) in [
        (
            None,
            Some(cursor("binding-current", 10, Some("answer"))),
            "head",
        ),
        (Some(cursor("binding-current", 0, None)), None, "tail"),
    ] {
        let error = PiTimelineAdapter::new()
            .resolve_user_entry(PiTurnUserEntryResolveRequest {
                source: source.clone(),
                session_id: "session-current".to_string(),
                turn_session_id: "session-current".to_string(),
                turn_id: "turn-incomplete".to_string(),
                is_first_session_turn: true,
                head_cursor,
                tail_cursor,
            })
            .unwrap_err();
        assert!(matches!(
            error,
            PiTurnUserEntryResolveError::BoundaryMissing { turn_id, boundary }
                if turn_id == "turn-incomplete" && boundary == expected_boundary
        ));
    }
}

#[test]
fn classifies_an_unsupported_or_unavailable_current_source() {
    let dir = tempdir().unwrap();
    let missing_path = dir.path().join("missing.jsonl");
    let base_request = PiTurnUserEntryResolveRequest {
        source: ResolvedAgentBinding {
            id: "binding-current".to_string(),
            client_type: "pi".to_string(),
            format: "pi-jsonl".to_string(),
            path: missing_path,
            fingerprint: None,
        },
        session_id: "session-current".to_string(),
        turn_session_id: "session-current".to_string(),
        turn_id: "turn-target".to_string(),
        is_first_session_turn: true,
        head_cursor: Some(cursor("binding-current", 0, None)),
        tail_cursor: Some(cursor("binding-current", 1, Some("user"))),
    };

    let unavailable = PiTimelineAdapter::new()
        .resolve_user_entry(base_request.clone())
        .unwrap_err();
    assert!(matches!(
        unavailable,
        PiTurnUserEntryResolveError::SourceUnavailable
    ));

    let unsupported = PiTimelineAdapter::new()
        .resolve_user_entry(PiTurnUserEntryResolveRequest {
            source: ResolvedAgentBinding {
                client_type: "claude".to_string(),
                ..base_request.source
            },
            ..base_request
        })
        .unwrap_err();
    assert!(matches!(
        unsupported,
        PiTurnUserEntryResolveError::SourceUnsupported
    ));
}

#[test]
fn rejects_ranges_with_no_primary_user_entry_or_multiple_user_entries() {
    let cases = [
        (
            concat!(
                "{\"type\":\"message\",\"id\":\"assistant\",\"parentId\":null,\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"orphan\"}]}}\n"
            ),
            "assistant",
            false,
        ),
        (
            concat!(
                "{\"type\":\"message\",\"id\":\"user-one\",\"parentId\":null,\"message\":{\"role\":\"user\",\"content\":\"one\"}}\n",
                "{\"type\":\"message\",\"id\":\"user-two\",\"parentId\":\"user-one\",\"message\":{\"role\":\"user\",\"content\":\"two\"}}\n",
                "{\"type\":\"message\",\"id\":\"answer\",\"parentId\":\"user-two\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"done\"}]}}\n"
            ),
            "answer",
            true,
        ),
    ];

    for (contents, terminal_id, ambiguous) in cases {
        let dir = tempdir().unwrap();
        let path = dir.path().join("session.jsonl");
        fs::write(&path, contents).unwrap();
        let error = PiTimelineAdapter::new()
            .resolve_user_entry(PiTurnUserEntryResolveRequest {
                source: ResolvedAgentBinding {
                    id: "binding-current".to_string(),
                    client_type: "pi".to_string(),
                    format: "pi-jsonl".to_string(),
                    path,
                    fingerprint: None,
                },
                session_id: "session-current".to_string(),
                turn_session_id: "session-current".to_string(),
                turn_id: "turn-target".to_string(),
                is_first_session_turn: true,
                head_cursor: Some(cursor("binding-current", 0, None)),
                tail_cursor: Some(cursor("binding-current", contents.len(), Some(terminal_id))),
            })
            .unwrap_err();
        if ambiguous {
            assert!(matches!(
                error,
                PiTurnUserEntryResolveError::UserEntryAmbiguous { turn_id }
                    if turn_id == "turn-target"
            ));
        } else {
            assert!(matches!(
                error,
                PiTurnUserEntryResolveError::UserEntryMissing { turn_id }
                    if turn_id == "turn-target"
            ));
        }
    }
}

#[test]
fn resolves_a_non_root_turn_from_its_semantic_chain_and_ignores_other_branches() {
    let previous = "{\"type\":\"message\",\"id\":\"previous\",\"parentId\":null,\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"previous\"}]}}\n";
    let target = concat!(
        "{\"type\":\"message\",\"id\":\"target-user\",\"parentId\":\"previous\",\"message\":{\"role\":\"user\",\"content\":\"target\"}}\n",
        "{\"type\":\"message\",\"id\":\"other-user\",\"parentId\":\"previous\",\"message\":{\"role\":\"user\",\"content\":\"other branch\"}}\n",
        "{\"type\":\"message\",\"id\":\"target-answer\",\"parentId\":\"target-user\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"answer\"}]}}\n"
    );
    let contents = format!("{previous}{target}");
    let dir = tempdir().unwrap();
    let path = dir.path().join("session.jsonl");
    fs::write(&path, &contents).unwrap();

    let resolved = PiTimelineAdapter::new()
        .resolve_user_entry(PiTurnUserEntryResolveRequest {
            source: ResolvedAgentBinding {
                id: "binding-current".to_string(),
                client_type: "pi".to_string(),
                format: "pi-jsonl".to_string(),
                path,
                fingerprint: None,
            },
            session_id: "session-current".to_string(),
            turn_session_id: "session-current".to_string(),
            turn_id: "turn-middle-or-abandoned".to_string(),
            is_first_session_turn: false,
            head_cursor: Some(cursor("binding-current", previous.len(), Some("previous"))),
            tail_cursor: Some(cursor(
                "binding-current",
                contents.len(),
                Some("target-answer"),
            )),
        })
        .unwrap();

    assert_eq!(resolved.entry_id, "target-user");
}

#[test]
fn classifies_malformed_physical_and_semantic_ranges_as_invalid() {
    let contents = "{\"type\":\"message\",\"id\":\"user\",\"parentId\":null,\"message\":{\"role\":\"user\",\"content\":\"hello\"}}\n";
    let dir = tempdir().unwrap();
    let path = dir.path().join("session.jsonl");
    fs::write(&path, contents).unwrap();
    let source = ResolvedAgentBinding {
        id: "binding-current".to_string(),
        client_type: "pi".to_string(),
        format: "pi-jsonl".to_string(),
        path,
        fingerprint: None,
    };

    for (head_cursor, tail_cursor) in [
        (
            cursor("binding-current", contents.len(), None),
            cursor("binding-current", 0, Some("user")),
        ),
        (
            cursor("binding-current", 0, Some("missing-parent")),
            cursor("binding-current", contents.len(), Some("user")),
        ),
        (
            cursor("binding-current", 0, None),
            cursor("binding-current", contents.len(), None),
        ),
    ] {
        let error = PiTimelineAdapter::new()
            .resolve_user_entry(PiTurnUserEntryResolveRequest {
                source: source.clone(),
                session_id: "session-current".to_string(),
                turn_session_id: "session-current".to_string(),
                turn_id: "turn-invalid".to_string(),
                is_first_session_turn: true,
                head_cursor: Some(head_cursor),
                tail_cursor: Some(tail_cursor),
            })
            .unwrap_err();
        assert!(matches!(
            error,
            PiTurnUserEntryResolveError::InvalidRange { turn_id }
                if turn_id == "turn-invalid"
        ));
    }
}
