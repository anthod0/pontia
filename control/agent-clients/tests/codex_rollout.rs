use pontia_agent_clients::{codex::rollout::CodexRollout, raw_transcripts::*};
use serde_json::{Value, json};
use std::io::Write;

fn fixture(records: &[Value]) -> (tempfile::TempDir, ResolvedAgentBinding) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("rollout.jsonl");
    let mut file = std::fs::File::create(&path).unwrap();
    writeln!(
        file,
        "{}",
        json!({"type":"session_meta","payload":{"id":"thread-a","cli_version":"0.155.1"}})
    )
    .unwrap();
    for record in records {
        writeln!(file, "{record}").unwrap();
    }
    let source = CodexRollout
        .resolve(&AgentBindingResolveRequest {
            id: "binding".into(),
            session_id: "session".into(),
            client_type: "codex".into(),
            client_session_file: Some(path),
        })
        .unwrap();
    (root, source)
}

fn context(turn: &str) -> Value {
    json!({"type":"turn_context","payload":{"turn_id":turn}})
}
fn item(payload: Value) -> Value {
    json!({"type":"response_item","payload":payload})
}
fn read(source: ResolvedAgentBinding, native: &str) -> Vec<TurnTimelineItem> {
    let capture = |kind| {
        CodexRollout
            .capture_boundary(TimelineBoundaryCaptureRequest {
                source: source.clone(),
                kind,
                native_entry_anchor: Some(native.into()),
                allow_missing_native_entry_anchor: false,
            })
            .unwrap()
            .cursor
    };
    let head = capture(TimelineBoundaryCaptureKind::Head);
    let tail = capture(TimelineBoundaryCaptureKind::Tail);
    CodexRollout
        .read_turn_ranges(TurnTimelineReadRequest {
            source,
            ranges: vec![TurnTimelineRange {
                turn_id: "pontia-turn".into(),
                is_first_session_turn: true,
                head_cursor: head,
                tail_cursor: Some(tail),
            }],
        })
        .unwrap()
}

#[test]
fn native_call_identity_associates_actual_answers_without_crossing_turns() {
    let (_root, source) = fixture(&[
        context("one"),
        item(
            json!({"type":"function_call","name":"request_user_input","call_id":"question","arguments":"{\"questions\":[{\"question\":\"Color?\"}]}"}),
        ),
        item(
            json!({"type":"function_call_output","call_id":"question","output":"{\"answers\":{\"color\":{\"answers\":[\"Blue\"]}}}"}),
        ),
        context("two"),
        item(
            json!({"type":"message","role":"user","content":[{"type":"input_text","text":"ordinary reply"}]}),
        ),
        item(json!({"type":"function_call_output","call_id":"question","output":"unrelated"})),
    ]);
    let first = read(source.clone(), "one");
    assert_eq!(first.len(), 2);
    assert_eq!(
        first[1].item.title.as_deref(),
        Some("Questions and answers")
    );
    assert!(first[1].item.content_preview.contains("Color?"));
    assert!(first[1].item.content_preview.contains("Blue"));
    let second = read(source, "two");
    assert_eq!(second[0].item.role, "user");
    assert_eq!(second[0].item.content_preview, "ordinary reply");
    assert_ne!(
        second[1].item.title.as_deref(),
        Some("Questions and answers")
    );
}

#[test]
fn preserves_long_tool_output_unknown_items_and_explicit_turn_metadata() {
    let long = "result λ\n".repeat(20_000);
    let (_root, source) = fixture(&[
        context("one"),
        item(
            json!({"type":"custom_tool_call","name":"code_mode","call_id":"code","input":"return 42"}),
        ),
        context("two"),
        item(
            json!({"type":"custom_tool_call_output","call_id":"code","output":long,"internal_chat_message_metadata_passthrough":{"turn_id":"one"}}),
        ),
        item(
            json!({"type":"new_native_item","value":{"nested":"preserved"},"internal_chat_message_metadata_passthrough":{"turn_id":"one"}}),
        ),
    ]);
    let items = read(source, "one");
    assert_eq!(items[1].item.content_preview, long);
    assert!(items[2].item.content_preview.contains("preserved"));
}

#[test]
fn rejects_truncated_or_replaced_sources_and_ignores_partial_final_record() {
    let (_root, source) = fixture(&[
        context("one"),
        item(json!({"type":"message","role":"assistant","content":[{"text":"done"}]})),
    ]);
    let capture = |kind| {
        CodexRollout
            .capture_boundary(TimelineBoundaryCaptureRequest {
                source: source.clone(),
                kind,
                native_entry_anchor: Some("one".into()),
                allow_missing_native_entry_anchor: false,
            })
            .unwrap()
            .cursor
    };
    let range = TurnTimelineRange {
        turn_id: "pontia-turn".into(),
        is_first_session_turn: true,
        head_cursor: capture(TimelineBoundaryCaptureKind::Head),
        tail_cursor: Some(capture(TimelineBoundaryCaptureKind::Tail)),
    };
    std::fs::OpenOptions::new()
        .append(true)
        .open(&source.path)
        .unwrap()
        .write_all(b"{\"type\":")
        .unwrap();
    assert_eq!(read(source.clone(), "one").len(), 1);
    std::fs::OpenOptions::new()
        .write(true)
        .open(&source.path)
        .unwrap()
        .set_len(95)
        .unwrap();
    assert!(
        CodexRollout
            .read_turn_ranges(TurnTimelineReadRequest {
                source,
                ranges: vec![range]
            })
            .is_err()
    );
}

#[test]
fn reads_native_command_patch_mcp_code_mode_and_tui_answer_records() {
    // Excerpts from isolated 0.155.1 app-server/TUI probes, with only turn ids
    // retained from environment-bearing turn_context records.
    let records: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/codex-0.155.1-native-details.json")).unwrap();
    let (_root, source) = fixture(&records);
    let cases = [
        (
            "01a0c4b8-0eb2-7121-a0a9-45fff7b7e4f0",
            "APPROVAL_OK",
            "CommandExecution",
        ),
        (
            "01a0c4b9-cdd4-7b43-b619-b642a6bba68b",
            "PATCH_APPROVAL_OK",
            "FileChange",
        ),
        (
            "01a0c4bb-9bf5-7561-a79b-6f0cfdffd174",
            "blue",
            "McpToolCall",
        ),
        (
            "01a0c759-24ad-75c2-a9bf-c0694427d47d",
            "Blue",
            "function_call_output",
        ),
    ];
    for (turn, text, kind) in cases {
        let items = read(source.clone(), turn);
        assert!(
            items
                .iter()
                .any(|item| item.item.raw_kind.as_deref() == Some(kind)
                    && item.item.content_preview.contains(text)),
            "missing {kind} native content"
        );
    }
    let complex = read(source.clone(), "01a0c7c9-a719-73e2-b50a-3605fed2a78a");
    assert!(complex.iter().any(|item| {
        item.item.raw_kind.as_deref() == Some("CommandExecution")
            && item
                .item
                .content_preview
                .contains(&format!("{}LONG_OUTPUT_END", "X".repeat(20_000)))
    }));
    assert!(complex.iter().any(|item| {
        item.item.raw_kind.as_deref() == Some("SubAgentActivity")
            && item
                .item
                .content_preview
                .contains("01a0c7c9-dcae-7920-8b33-54a172b95ad5")
    }));
    assert!(complex.iter().any(
        |item| item.item.raw_kind.as_deref() == Some("agent_message")
            && item.item.content_preview.contains("CHILD_NATIVE_OK")
    ));
    let answers = read(source, "01a0c759-24ad-75c2-a9bf-c0694427d47d");
    assert!(answers.iter().any(
        |item| item.item.title.as_deref() == Some("Questions and answers")
            && item.item.content_preview.contains("Choose Red or Blue?")
    ));
}

#[test]
fn refuses_missing_completed_native_turn_instead_of_returning_empty_history() {
    let (_root, source) = fixture(&[context("one")]);
    assert!(
        CodexRollout
            .capture_boundary(TimelineBoundaryCaptureRequest {
                source: source.clone(),
                kind: TimelineBoundaryCaptureKind::Head,
                native_entry_anchor: Some("missing".into()),
                allow_missing_native_entry_anchor: false,
            })
            .is_err()
    );
    let head = CodexRollout
        .capture_source_origin_head(&source.id, Some("missing".into()))
        .unwrap()
        .cursor;
    let tail = CodexRollout
        .capture_boundary(TimelineBoundaryCaptureRequest {
            source: source.clone(),
            kind: TimelineBoundaryCaptureKind::Tail,
            native_entry_anchor: Some("missing".into()),
            allow_missing_native_entry_anchor: true,
        })
        .unwrap()
        .cursor;
    assert!(
        CodexRollout
            .read_turn_ranges(TurnTimelineReadRequest {
                source,
                ranges: vec![TurnTimelineRange {
                    turn_id: "turn".into(),
                    is_first_session_turn: true,
                    head_cursor: head,
                    tail_cursor: Some(tail)
                }],
            })
            .is_err()
    );
}
