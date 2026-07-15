use std::fs;

use pontia_agent_clients::pi::raw_transcripts::{
    PiJsonlV2Cursor, PiTimelineAdapter, TimelineBoundaryRelation,
};
use pontia_agent_clients::raw_transcripts::{
    ResolvedAgentBinding, TimelineBoundaryCaptureKind, TimelineBoundaryCaptureRequest,
    TimelineBoundaryCapturer,
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
