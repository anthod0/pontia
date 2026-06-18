use std::{fs, io::Write};

use pontia::application::AgentBinding;
use pontia_agent_clients::{
    pi::raw_transcripts::{PiAgentBindingResolver, PiJsonlParser},
    raw_transcripts::{
        AgentBindingResolveRequest, AgentBindingResolver, RawTranscriptParser, TimelinePageRequest,
    },
};
use serde_json::json;
use tempfile::tempdir;

fn pi_session_dir(agent_dir: &std::path::Path, cwd: &str) -> std::path::PathBuf {
    let safe = format!(
        "--{}--",
        cwd.trim_start_matches('/').replace(['/', '\\', ':'], "-")
    );
    agent_dir.join("sessions").join(safe)
}

#[test]
fn pi_resolver_finds_jsonl_for_launch_cwd_and_client_session_key() {
    let temp = tempdir().unwrap();
    let agent_dir = temp.path().join("agent");
    let cwd = temp.path().join("workspace");
    fs::create_dir_all(&cwd).unwrap();
    let cwd = cwd.canonicalize().unwrap();
    let session_key = "11111111-2222-3333-4444-555555555555";
    let session_dir = pi_session_dir(&agent_dir, cwd.to_str().unwrap());
    fs::create_dir_all(&session_dir).unwrap();
    let session_file = session_dir.join(format!("2026-06-09T00-00-00-000Z_{session_key}.jsonl"));
    fs::write(&session_file, "{\"type\":\"session\",\"version\":3}\n").unwrap();

    let resolver = PiAgentBindingResolver::with_agent_dir(agent_dir.clone());
    let source = resolver
        .resolve(&AgentBindingResolveRequest {
            id: "bind_1".to_string(),
            session_id: "sess_1".to_string(),
            client_type: "pi".to_string(),
            launch_cwd: cwd.clone(),
            client_session_key: session_key.to_string(),
        })
        .unwrap();

    assert_eq!(source.id, "bind_1");
    assert_eq!(source.client_type, "pi");
    assert_eq!(source.format, "pi-jsonl");
    assert_eq!(source.path, session_file);
}

#[test]
fn pi_parser_returns_recent_conversation_rounds_then_older_rounds() {
    let temp = tempdir().unwrap();
    let session_file = temp.path().join("session.jsonl");
    fs::write(
        &session_file,
        concat!(
            "{\"type\":\"session\",\"version\":3,\"id\":\"sess\",\"timestamp\":\"2026-06-09T00:00:00.000Z\"}\n",
            "{\"type\":\"message\",\"id\":\"u1\",\"timestamp\":\"2026-06-09T00:00:01.000Z\",\"message\":{\"role\":\"user\",\"content\":\"first user\"}}\n",
            "{\"type\":\"message\",\"id\":\"a1\",\"timestamp\":\"2026-06-09T00:00:02.000Z\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"first assistant\"}]}}\n",
            "{\"type\":\"message\",\"id\":\"u2\",\"timestamp\":\"2026-06-09T00:00:03.000Z\",\"message\":{\"role\":\"user\",\"content\":\"second user\"}}\n",
            "{\"type\":\"message\",\"id\":\"a2\",\"timestamp\":\"2026-06-09T00:00:04.000Z\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"thinking\",\"thinking\":\"plan\"},{\"type\":\"text\",\"text\":\"second assistant\"}]}}\n",
            "{\"type\":\"message\",\"id\":\"t2\",\"timestamp\":\"2026-06-09T00:00:05.000Z\",\"message\":{\"role\":\"toolResult\",\"toolName\":\"read\",\"content\":[{\"type\":\"text\",\"text\":\"file contents\"}],\"isError\":false}}\n",
            "{\"type\":\"message\",\"id\":\"u3\",\"timestamp\":\"2026-06-09T00:00:06.000Z\",\"message\":{\"role\":\"user\",\"content\":\"third user\"}}\n",
            "{\"type\":\"message\",\"id\":\"a3\",\"timestamp\":\"2026-06-09T00:00:07.000Z\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"third assistant\"}]}}\n",
            "{\"type\":\"message\",\"id\":\"u4\",\"timestamp\":\"2026-06-09T00:00:08.000Z\",\"message\":{\"role\":\"user\",\"content\":\"fourth user\"}}\n",
            "{\"type\":\"message\",\"id\":\"a4\",\"timestamp\":\"2026-06-09T00:00:09.000Z\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"fourth assistant\"}]}}\n",
        ),
    )
    .unwrap();

    let source = pontia::application::ResolvedAgentBinding {
        id: "bind_1".to_string(),
        client_type: "pi".to_string(),
        format: "pi-jsonl".to_string(),
        path: session_file,
        fingerprint: None,
    };
    let parser = PiJsonlParser::new();

    let recent_page = parser
        .timeline_page(TimelinePageRequest {
            session_id: "sess_1".to_string(),
            source: source.clone(),
            before: None,
            after: None,
            limit: Some(3),
        })
        .unwrap();

    assert_eq!(recent_page.binding_id, "bind_1");
    let recent_previews: Vec<_> = recent_page
        .items
        .iter()
        .filter(|item| matches!(item.kind.as_str(), "user" | "assistant"))
        .map(|item| item.content_preview.as_str())
        .collect();
    assert_eq!(
        recent_previews,
        vec![
            "second user",
            "second assistant",
            "third user",
            "third assistant",
            "fourth user",
            "fourth assistant",
        ]
    );
    assert_eq!(recent_page.items[1].kind, "thinking");
    assert!(
        recent_page
            .items
            .iter()
            .any(|item| item.kind == "tool_result")
    );
    assert!(recent_page.has_more);
    let cursor = recent_page.head_cursor.clone().unwrap();
    assert!(recent_page.tail_cursor.is_some());

    let older_page = parser
        .timeline_page(TimelinePageRequest {
            session_id: "sess_1".to_string(),
            source,
            before: Some(cursor),
            after: None,
            limit: Some(3),
        })
        .unwrap();

    let older_previews: Vec<_> = older_page
        .items
        .iter()
        .filter(|item| matches!(item.kind.as_str(), "user" | "assistant"))
        .map(|item| item.content_preview.as_str())
        .collect();
    assert_eq!(older_previews, vec!["first user", "first assistant"]);
    assert!(!older_page.has_more);
}

#[test]
fn pi_parser_returns_updates_after_tail_cursor_without_reverse_anchor_scan() {
    let temp = tempdir().unwrap();
    let session_file = temp.path().join("session.jsonl");
    fs::write(
        &session_file,
        concat!(
            "{\"type\":\"message\",\"id\":\"u1\",\"timestamp\":\"2026-06-09T00:00:01.000Z\",\"message\":{\"role\":\"user\",\"content\":\"first user\"}}\n",
            "{\"type\":\"message\",\"id\":\"a1\",\"timestamp\":\"2026-06-09T00:00:02.000Z\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"first assistant\"}]}}\n",
            "{\"type\":\"message\",\"id\":\"u2\",\"timestamp\":\"2026-06-09T00:00:03.000Z\",\"message\":{\"role\":\"user\",\"content\":\"second user\"}}\n",
            "{\"type\":\"message\",\"id\":\"a2\",\"timestamp\":\"2026-06-09T00:00:04.000Z\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"second assistant\"}]}}\n",
        ),
    )
    .unwrap();

    let source = pontia::application::ResolvedAgentBinding {
        id: "bind_1".to_string(),
        client_type: "pi".to_string(),
        format: "pi-jsonl".to_string(),
        path: session_file,
        fingerprint: None,
    };
    let parser = PiJsonlParser::new();

    let initial = parser
        .timeline_page(TimelinePageRequest {
            session_id: "sess_1".to_string(),
            source: source.clone(),
            before: None,
            after: None,
            limit: Some(1),
        })
        .unwrap();
    let tail_cursor = initial.tail_cursor.clone().unwrap();

    fs::OpenOptions::new()
        .append(true)
        .open(&source.path)
        .unwrap()
        .write_all(
            concat!(
                "{\"type\":\"message\",\"id\":\"u3\",\"timestamp\":\"2026-06-09T00:00:05.000Z\",\"message\":{\"role\":\"user\",\"content\":\"third user\"}}\n",
                "{\"type\":\"message\",\"id\":\"a3\",\"timestamp\":\"2026-06-09T00:00:06.000Z\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"third assistant\"}]}}\n",
            )
            .as_bytes(),
        )
        .unwrap();

    let updates = parser
        .timeline_page(TimelinePageRequest {
            session_id: "sess_1".to_string(),
            source,
            before: None,
            after: Some(tail_cursor),
            limit: None,
        })
        .unwrap();

    assert_eq!(
        updates
            .items
            .iter()
            .map(|item| item.item_id.as_str())
            .collect::<Vec<_>>(),
        vec!["pi:entry:u3:block:0", "pi:entry:a3:block:0"]
    );
    assert!(!updates.has_more);
    assert!(updates.tail_cursor.is_some());
}

#[test]
fn pi_parser_parses_started_managed_tool_uses_into_structured_inputs() {
    let temp = tempdir().unwrap();
    let session_file = temp.path().join("session.jsonl");
    let lines = [
        json!({
            "type": "message",
            "id": "a1",
            "timestamp": "2026-06-09T00:00:01.000Z",
            "message": {
                "role": "assistant",
                "content": [
                    {"type":"toolCall", "name":"read", "arguments":{"path":"src/main.rs", "start_line": 10, "end_line": 20}},
                    {"type":"toolCall", "name":"edit", "arguments":{"path":"src/main.rs", "edits":[{"oldText":"old", "newText":"new"}]}},
                    {"type":"toolCall", "name":"write", "arguments":{"path":"README.md", "content":"hello"}},
                    {"type":"toolCall", "name":"bash", "arguments":{"command":"cargo test", "timeout": 120}},
                    {"type":"toolCall", "name":"unknown", "arguments":{"value":true}}
                ],
            },
        }),
        json!({
            "type": "message",
            "id": "t1",
            "timestamp": "2026-06-09T00:00:02.000Z",
            "message": {"role":"toolResult", "toolName":"read", "content":"done", "isError":false},
        }),
    ]
    .into_iter()
    .map(|line| line.to_string())
    .collect::<Vec<_>>()
    .join("\n");
    fs::write(&session_file, format!("{lines}\n")).unwrap();

    let source = pontia::application::ResolvedAgentBinding {
        id: "bind_1".to_string(),
        client_type: "pi".to_string(),
        format: "pi-jsonl".to_string(),
        path: session_file,
        fingerprint: None,
    };
    let parser = PiJsonlParser::new();

    let page = parser
        .timeline_page(TimelinePageRequest {
            session_id: "sess_1".to_string(),
            source,
            before: None,
            after: None,
            limit: Some(10),
        })
        .unwrap();

    let tool_uses = page
        .items
        .iter()
        .filter(|item| item.kind == "tool_call")
        .map(|item| {
            item.managed_tool_use
                .as_ref()
                .map(|tool| serde_json::to_value(tool).unwrap())
        })
        .collect::<Vec<_>>();

    assert_eq!(
        tool_uses,
        vec![
            Some(
                json!({"tool_name":"read","input":{"type":"read","path":"src/main.rs","start_line":10,"end_line":20}})
            ),
            Some(
                json!({"tool_name":"edit","input":{"type":"edit","path":"src/main.rs","edits_count":1}})
            ),
            Some(json!({"tool_name":"write","input":{"type":"write","path":"README.md"}})),
            Some(
                json!({"tool_name":"bash","input":{"type":"bash","command":"cargo test","timeout":120}})
            ),
            None,
        ]
    );
    assert!(
        page.items
            .iter()
            .find(|item| item.kind == "tool_result")
            .unwrap()
            .managed_tool_use
            .is_none()
    );
}

#[test]
fn pi_parser_keeps_user_and_assistant_previews_full_but_truncates_other_kinds() {
    let temp = tempdir().unwrap();
    let session_file = temp.path().join("session.jsonl");
    let long_user = "u".repeat(260);
    let long_assistant = "a".repeat(260);
    let long_thinking = "t".repeat(260);
    let lines = [
        json!({
            "type": "message",
            "id": "u1",
            "timestamp": "2026-06-09T00:00:01.000Z",
            "message": {"role": "user", "content": long_user},
        }),
        json!({
            "type": "message",
            "id": "a1",
            "timestamp": "2026-06-09T00:00:02.000Z",
            "message": {
                "role": "assistant",
                "content": [
                    {"type": "thinking", "thinking": long_thinking},
                    {"type": "text", "text": long_assistant},
                ],
            },
        }),
    ]
    .into_iter()
    .map(|line| line.to_string())
    .collect::<Vec<_>>()
    .join("\n");
    fs::write(&session_file, format!("{lines}\n")).unwrap();

    let source = pontia::application::ResolvedAgentBinding {
        id: "bind_1".to_string(),
        client_type: "pi".to_string(),
        format: "pi-jsonl".to_string(),
        path: session_file,
        fingerprint: None,
    };
    let parser = PiJsonlParser::new();

    let page = parser
        .timeline_page(TimelinePageRequest {
            session_id: "sess_1".to_string(),
            source,
            before: None,
            after: None,
            limit: Some(10),
        })
        .unwrap();

    assert_eq!(page.items[0].kind, "user");
    assert_eq!(page.items[0].content_preview, "u".repeat(260));
    assert_eq!(page.items[1].kind, "thinking");
    assert_eq!(
        page.items[1].content_preview,
        format!("{}…", "t".repeat(240))
    );
    assert_eq!(page.items[2].kind, "assistant");
    assert_eq!(page.items[2].content_preview, "a".repeat(260));
}

#[test]
fn pi_parser_falls_back_to_raw_kind_for_unmapped_message_roles() {
    let temp = tempdir().unwrap();
    let session_file = temp.path().join("session.jsonl");
    fs::write(
        &session_file,
        "{\"type\":\"message\",\"id\":\"x1\",\"timestamp\":\"2026-06-09T00:00:01.000Z\",\"message\":{\"role\":\"vendorSpecial\",\"content\":\"raw payload\"}}\n",
    )
    .unwrap();

    let source = pontia::application::ResolvedAgentBinding {
        id: "bind_1".to_string(),
        client_type: "pi".to_string(),
        format: "pi-jsonl".to_string(),
        path: session_file,
        fingerprint: None,
    };
    let parser = PiJsonlParser::new();

    let page = parser
        .timeline_page(TimelinePageRequest {
            session_id: "sess_1".to_string(),
            source,
            before: None,
            after: None,
            limit: Some(10),
        })
        .unwrap();

    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].kind, "vendorSpecial");
    assert_eq!(page.items[0].raw_kind.as_deref(), Some("vendorSpecial"));
    assert_eq!(page.items[0].content_preview, "raw payload");
}

#[tokio::test]
async fn service_can_resolve_and_parse_primary_binding_for_session() {
    let temp = tempdir().unwrap();
    let agent_dir = temp.path().join("agent");
    let cwd = temp.path().join("workspace");
    fs::create_dir_all(&cwd).unwrap();
    let cwd = cwd.canonicalize().unwrap();
    let session_key = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";
    let session_dir = pi_session_dir(&agent_dir, cwd.to_str().unwrap());
    fs::create_dir_all(&session_dir).unwrap();
    fs::write(
        session_dir.join(format!("2026-06-09T00-00-00-000Z_{session_key}.jsonl")),
        "{\"type\":\"message\",\"id\":\"u1\",\"timestamp\":\"2026-06-09T00:00:01.000Z\",\"message\":{\"role\":\"user\",\"content\":\"hi\"}}\n",
    )
    .unwrap();

    let binding = AgentBinding {
        id: "bind_1".to_string(),
        session_id: "sess_1".to_string(),
        client_type: "pi".to_string(),
        launch_cwd: cwd.to_string_lossy().to_string(),
        client_session_key: session_key.to_string(),
        metadata: json!({}),
        discovered: false,
    };
    let resolver = PiAgentBindingResolver::with_agent_dir(agent_dir);
    let parser = PiJsonlParser::new();

    let page = pontia::application::resolve_and_parse_timeline_page(
        &binding, &resolver, &parser, None, 20,
    )
    .await
    .unwrap();

    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].kind, "user");
}
