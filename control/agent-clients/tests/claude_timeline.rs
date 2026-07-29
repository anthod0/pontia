use std::fs;

use pontia_agent_clients::{
    claude::raw_transcripts::{
        ClaudeAgentBindingResolver, ClaudeJsonlV1Cursor, ClaudeTimelineAdapter,
    },
    raw_transcripts::{
        AgentBindingResolveRequest, AgentBindingResolver, ResolvedAgentBinding,
        TimelineBoundaryCaptureKind, TimelineBoundaryCaptureRequest, TimelineBoundaryCapturer,
        TurnTimelineRange, TurnTimelineReadRequest, TurnTimelineReader,
    },
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
            client_type: "claude".to_string(),
            format: "claude-jsonl".to_string(),
            path,
            fingerprint: None,
        },
    )
}

fn cursor(offset: usize) -> String {
    ClaudeJsonlV1Cursor {
        binding_id: "binding_1".to_string(),
        byte_offset: offset,
    }
    .encode()
}

#[test]
fn claude_resolver_uses_the_hook_reported_transcript_path() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("arbitrary-name.jsonl");
    fs::write(&path, "").unwrap();

    let source = ClaudeAgentBindingResolver::new()
        .resolve(&AgentBindingResolveRequest {
            id: "binding_1".to_string(),
            session_id: "session_1".to_string(),
            client_type: "claude".to_string(),
            client_session_file: Some(path.clone()),
        })
        .unwrap();

    assert_eq!(source.path, path);
    assert_eq!(source.format, "claude-jsonl");
}

#[test]
fn claude_head_capture_includes_a_prompt_already_written_before_the_hook() {
    let previous = b"{\"type\":\"system\",\"subtype\":\"turn_duration\"}\n";
    let prompt = b"{\"type\":\"user\",\"uuid\":\"user-1\",\"message\":{\"role\":\"user\",\"content\":\"hello\"}}\n";
    let contents = [previous.as_slice(), prompt.as_slice()].concat();
    let (_dir, source) = source(&contents);

    let boundary = ClaudeTimelineAdapter::new()
        .capture_boundary(TimelineBoundaryCaptureRequest {
            source,
            kind: TimelineBoundaryCaptureKind::Head,
            native_entry_anchor: None,
            allow_missing_native_entry_anchor: false,
        })
        .unwrap();

    assert_eq!(boundary.cursor, cursor(previous.len()));
}

#[test]
fn claude_linear_reader_maps_messages_thinking_and_tools_without_using_topology() {
    let user = b"{\"type\":\"user\",\"uuid\":\"u1\",\"timestamp\":\"2026-01-01T00:00:00Z\",\"message\":{\"role\":\"user\",\"content\":\"question\"}}\n";
    let assistant = b"{\"type\":\"assistant\",\"uuid\":\"a1\",\"parentUuid\":\"unrelated-native-parent\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"thinking\",\"thinking\":\"inspect\"},{\"type\":\"tool_use\",\"id\":\"tool-1\",\"name\":\"Read\",\"input\":{\"file_path\":\"README.md\",\"offset\":2,\"limit\":3}},{\"type\":\"text\",\"text\":\"answer\"}]}}\n";
    let tool_result = b"{\"type\":\"user\",\"uuid\":\"r1\",\"isSidechain\":false,\"message\":{\"role\":\"user\",\"content\":[{\"type\":\"tool_result\",\"tool_use_id\":\"tool-1\",\"content\":\"file contents\",\"is_error\":false}]}}\n";
    let sidechain = b"{\"type\":\"assistant\",\"uuid\":\"side\",\"isSidechain\":true,\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"hidden subagent\"}]}}\n";
    let contents = [
        user.as_slice(),
        assistant.as_slice(),
        tool_result.as_slice(),
        sidechain.as_slice(),
    ]
    .concat();
    let (_dir, source) = source(&contents);

    let items = ClaudeTimelineAdapter::new()
        .read_turn_ranges(TurnTimelineReadRequest {
            source,
            ranges: vec![TurnTimelineRange {
                turn_id: "turn_1".to_string(),
                is_first_session_turn: true,
                head_cursor: cursor(0),
                tail_cursor: Some(cursor(contents.len())),
            }],
        })
        .unwrap();

    assert_eq!(items.len(), 5);
    assert_eq!(items[0].item.kind, "user");
    assert_eq!(items[1].item.kind, "thinking");
    assert_eq!(items[2].item.kind, "tool_call");
    assert_eq!(items[2].item.title.as_deref(), Some("Read"));
    assert_eq!(
        items[2]
            .item
            .managed_tool_use
            .as_ref()
            .map(|tool| tool.tool_name.as_str()),
        Some("Read")
    );
    assert_eq!(items[3].item.kind, "assistant");
    assert_eq!(items[4].item.kind, "tool_result");
    assert!(items.iter().all(|item| item.turn_id == "turn_1"));
}

#[test]
fn active_claude_reader_ignores_only_an_incomplete_trailing_record() {
    let complete = b"{\"type\":\"assistant\",\"uuid\":\"a1\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"partial answer\"}]}}\n";
    let incomplete = b"{\"type\":\"assistant\"";
    let contents = [complete.as_slice(), incomplete.as_slice()].concat();
    let (_dir, source) = source(&contents);

    let items = ClaudeTimelineAdapter::new()
        .read_turn_ranges(TurnTimelineReadRequest {
            source: source.clone(),
            ranges: vec![TurnTimelineRange {
                turn_id: "turn_active".to_string(),
                is_first_session_turn: true,
                head_cursor: cursor(0),
                tail_cursor: None,
            }],
        })
        .unwrap();
    assert_eq!(items.len(), 1);

    let error = ClaudeTimelineAdapter::new()
        .read_turn_ranges(TurnTimelineReadRequest {
            source,
            ranges: vec![TurnTimelineRange {
                turn_id: "turn_sealed".to_string(),
                is_first_session_turn: true,
                head_cursor: cursor(0),
                tail_cursor: Some(cursor(contents.len())),
            }],
        })
        .unwrap_err();
    assert!(error.to_string().contains("incomplete JSONL"));
}
