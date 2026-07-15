use std::fs;

use pontia_agent_clients::pi::raw_transcripts::{
    PiJsonlV2Cursor, PiTimelineAdapter, TimelineBoundaryRelation,
};
use pontia_agent_clients::raw_transcripts::{
    ResolvedAgentBinding, TimelineBoundaryCaptureKind, TimelineBoundaryCaptureRequest,
    TimelineBoundaryCapturer, TurnTimelineRange, TurnTimelineReadRequest, TurnTimelineReader,
};
use tempfile::tempdir;

fn source(contents: &[u8]) -> (tempfile::TempDir, ResolvedAgentBinding) {
    let dir = tempdir().unwrap();
    let path = dir.path().join("session.jsonl");
    fs::write(&path, contents).unwrap();
    (
        dir,
        ResolvedAgentBinding {
            id: "binding_1".to_string(),
            client_type: "pi".to_string(),
            format: "pi-jsonl".to_string(),
            path,
            fingerprint: None,
        },
    )
}

fn cursor(offset: usize, anchor: Option<&str>) -> String {
    PiJsonlV2Cursor {
        binding_id: "binding_1".to_string(),
        byte_offset: offset,
        native_entry_anchor: anchor.map(ToString::to_string),
        relation: TimelineBoundaryRelation::After,
    }
    .encode()
}

#[test]
fn pi_v2_boundary_capture_combines_binding_eof_anchor_and_relation() {
    let (_dir, source) = source(b"{\"id\":\"previous\"}\n");
    let adapter = PiTimelineAdapter::new();

    let boundary = adapter
        .capture_boundary(TimelineBoundaryCaptureRequest {
            source,
            kind: TimelineBoundaryCaptureKind::Head,
            native_entry_anchor: Some("entry:previous".to_string()),
            allow_missing_native_entry_anchor: false,
        })
        .unwrap();

    assert_eq!(
        boundary.cursor,
        "pi-jsonl-v2:binding_1:18:after:entry:previous"
    );
    assert_eq!(
        PiJsonlV2Cursor::decode(&boundary.cursor, "binding_1").unwrap(),
        PiJsonlV2Cursor {
            binding_id: "binding_1".to_string(),
            byte_offset: 18,
            native_entry_anchor: Some("entry:previous".to_string()),
            relation: TimelineBoundaryRelation::After,
        }
    );
}

#[test]
fn pi_v2_cursor_rejects_v1_and_binding_scope_mismatches() {
    let v1 = PiJsonlV2Cursor::decode("pi-jsonl-v1:binding_1:18:0", "binding_1")
        .unwrap_err()
        .to_string();
    let wrong_scope =
        PiJsonlV2Cursor::decode("pi-jsonl-v2:binding_1:18:after:entry_1", "binding_2")
            .unwrap_err()
            .to_string();

    assert!(v1.contains("cursor format mismatch"));
    assert!(wrong_scope.contains("cursor scope mismatch"));
}

#[test]
fn pi_boundary_capture_only_allows_a_missing_anchor_for_session_start_heads() {
    let adapter = PiTimelineAdapter::new();
    let (_dir, source) = source(b"");
    let first_head = adapter
        .capture_boundary(TimelineBoundaryCaptureRequest {
            source: source.clone(),
            kind: TimelineBoundaryCaptureKind::Head,
            native_entry_anchor: None,
            allow_missing_native_entry_anchor: true,
        })
        .unwrap();
    assert_eq!(first_head.cursor, "pi-jsonl-v2:binding_1:0:after:");

    for (kind, allow_missing) in [
        (TimelineBoundaryCaptureKind::Head, false),
        (TimelineBoundaryCaptureKind::Tail, false),
        (TimelineBoundaryCaptureKind::Tail, true),
    ] {
        let error = adapter
            .capture_boundary(TimelineBoundaryCaptureRequest {
                source: source.clone(),
                kind,
                native_entry_anchor: None,
                allow_missing_native_entry_anchor: allow_missing,
            })
            .unwrap_err()
            .to_string();
        assert!(error.contains("native entry anchor is required"));
    }
}

#[test]
fn pi_turn_reader_restores_parent_chain_and_filters_unrelated_physical_entries() {
    let root = b"{\"type\":\"message\",\"id\":\"root\",\"parentId\":null}\n";
    let window = concat!(
        "{\"type\":\"message\",\"id\":\"user\",\"parentId\":\"root\",\"message\":{\"role\":\"user\",\"content\":\"hello\"}}\n",
        "{\"type\":\"message\",\"id\":\"other\",\"parentId\":\"root\",\"message\":{\"role\":\"user\",\"content\":\"other branch\"}}\n",
        "{\"type\":\"message\",\"id\":\"answer\",\"parentId\":\"user\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"world\"}]}}\n"
    )
    .as_bytes();
    let contents = [root.as_slice(), window].concat();
    let (_dir, source) = source(&contents);

    let items = PiTimelineAdapter::new()
        .read_turn_ranges(TurnTimelineReadRequest {
            source,
            ranges: vec![TurnTimelineRange {
                turn_id: "turn_1".to_string(),
                turn_index: 1,
                head_cursor: cursor(root.len(), Some("root")),
                tail_cursor: Some(cursor(contents.len(), Some("answer"))),
            }],
        })
        .unwrap();

    assert_eq!(items.len(), 2);
    assert!(items.iter().all(|item| item.turn_id == "turn_1"));
    assert_eq!(items[0].item.content_preview, "hello");
    assert_eq!(items[1].item.content_preview, "world");
}

#[test]
fn pi_turn_reader_rejects_semantic_overlap_and_invalid_ranges() {
    let contents = b"{\"type\":\"message\",\"id\":\"entry\",\"parentId\":null,\"message\":{\"role\":\"user\",\"content\":\"hello\"}}\n";
    let (_dir, resolved_source) = source(contents);
    let range = TurnTimelineRange {
        turn_id: "turn_1".to_string(),
        turn_index: 1,
        head_cursor: cursor(0, None),
        tail_cursor: Some(cursor(contents.len(), Some("entry"))),
    };
    let overlap = PiTimelineAdapter::new()
        .read_turn_ranges(TurnTimelineReadRequest {
            source: resolved_source.clone(),
            ranges: vec![
                range.clone(),
                TurnTimelineRange {
                    turn_id: "turn_2".to_string(),
                    turn_index: 1,
                    ..range.clone()
                },
            ],
        })
        .unwrap_err()
        .to_string();
    assert!(overlap.contains("Turn turn_2"));
    assert!(overlap.contains("semantic Turn ranges overlap"));

    let null_later_head = PiTimelineAdapter::new()
        .read_turn_ranges(TurnTimelineReadRequest {
            source: resolved_source.clone(),
            ranges: vec![TurnTimelineRange {
                turn_id: "turn_later".to_string(),
                turn_index: 2,
                ..range.clone()
            }],
        })
        .unwrap_err()
        .to_string();
    assert!(null_later_head.contains("only the first Session Turn"));

    let cases = [
        (
            cursor(contents.len(), None),
            cursor(0, Some("entry")),
            "reversed or outside",
        ),
        (
            cursor(0, None),
            cursor(contents.len() + 1, Some("entry")),
            "reversed or outside",
        ),
        (
            cursor(0, None),
            cursor(contents.len(), None),
            "terminal native",
        ),
        (
            cursor(0, Some("missing_parent")),
            cursor(contents.len(), Some("entry")),
            "does not reach the head anchor",
        ),
    ];
    for (head_cursor, tail_cursor, expected) in cases {
        let error = PiTimelineAdapter::new()
            .read_turn_ranges(TurnTimelineReadRequest {
                source: resolved_source.clone(),
                ranges: vec![TurnTimelineRange {
                    turn_id: "turn_invalid".to_string(),
                    turn_index: 1,
                    head_cursor,
                    tail_cursor: Some(tail_cursor),
                }],
            })
            .unwrap_err()
            .to_string();
        assert!(error.contains("Turn turn_invalid"), "{error}");
        assert!(error.contains(expected), "{error}");
    }

    let incomplete = b"{\"id\":\"entry\",\"parentId\":null}";
    let (_dir, incomplete_source) = source(incomplete);
    let error = PiTimelineAdapter::new()
        .read_turn_ranges(TurnTimelineReadRequest {
            source: incomplete_source,
            ranges: vec![TurnTimelineRange {
                turn_id: "turn_incomplete".to_string(),
                turn_index: 1,
                head_cursor: cursor(0, None),
                tail_cursor: Some(cursor(incomplete.len(), Some("entry"))),
            }],
        })
        .unwrap_err()
        .to_string();
    assert!(error.contains("incomplete JSONL"), "{error}");
}
