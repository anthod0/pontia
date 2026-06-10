use std::fs;

use pilotfy::application::{
    AgentBinding, AgentBindingResolveRequest, AgentBindingResolver, PiAgentBindingResolver,
    PiJsonlParser, RawTranscriptParser, TimelinePageRequest,
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
fn pi_parser_returns_timeline_page_from_jsonl_with_cursor() {
    let temp = tempdir().unwrap();
    let session_file = temp.path().join("session.jsonl");
    fs::write(
        &session_file,
        concat!(
            "{\"type\":\"session\",\"version\":3,\"id\":\"sess\",\"timestamp\":\"2026-06-09T00:00:00.000Z\"}\n",
            "{\"type\":\"message\",\"id\":\"u1\",\"timestamp\":\"2026-06-09T00:00:01.000Z\",\"message\":{\"role\":\"user\",\"content\":\"hello world\",\"timestamp\":1780963201000}}\n",
            "{\"type\":\"message\",\"id\":\"a1\",\"timestamp\":\"2026-06-09T00:00:02.000Z\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"thinking\",\"thinking\":\"plan\"},{\"type\":\"text\",\"text\":\"answer\"},{\"type\":\"toolCall\",\"id\":\"call_1\",\"name\":\"read\",\"arguments\":{\"path\":\"src/main.rs\"}}],\"timestamp\":1780963202000}}\n",
            "{\"type\":\"message\",\"id\":\"t1\",\"timestamp\":\"2026-06-09T00:00:03.000Z\",\"message\":{\"role\":\"toolResult\",\"toolCallId\":\"call_1\",\"toolName\":\"read\",\"content\":[{\"type\":\"text\",\"text\":\"file contents\"}],\"isError\":false,\"timestamp\":1780963203000}}\n",
            "{\"type\":\"model_change\",\"id\":\"m1\",\"timestamp\":\"2026-06-09T00:00:04.000Z\",\"provider\":\"openai\",\"modelId\":\"gpt-4o\"}\n",
        ),
    )
    .unwrap();

    let source = pilotfy::application::ResolvedAgentBinding {
        id: "bind_1".to_string(),
        client_type: "pi".to_string(),
        format: "pi-jsonl".to_string(),
        path: session_file.clone(),
        fingerprint: None,
    };
    let parser = PiJsonlParser::new();

    let first_page = parser
        .timeline_page(TimelinePageRequest {
            session_id: "sess_1".to_string(),
            source: source.clone(),
            cursor: None,
            limit: 3,
        })
        .unwrap();

    assert_eq!(first_page.binding_id, "bind_1");
    assert_eq!(first_page.items.len(), 3);
    assert_eq!(first_page.items[0].kind, "user_message");
    assert_eq!(first_page.items[0].content_preview, "hello world");
    assert_eq!(first_page.items[1].kind, "assistant_thinking");
    assert_eq!(first_page.items[2].kind, "assistant_message");
    assert!(first_page.has_more);
    assert!(!first_page.is_tail);
    let cursor = first_page.next_cursor.clone().unwrap();

    let second_page = parser
        .timeline_page(TimelinePageRequest {
            session_id: "sess_1".to_string(),
            source,
            cursor: Some(cursor),
            limit: 10,
        })
        .unwrap();

    let kinds: Vec<_> = second_page
        .items
        .iter()
        .map(|item| item.kind.as_str())
        .collect();
    assert_eq!(kinds, vec!["tool_call", "tool_result", "model_change"]);
    assert!(!second_page.has_more);
    assert!(second_page.is_tail);
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
    };
    let resolver = PiAgentBindingResolver::with_agent_dir(agent_dir);
    let parser = PiJsonlParser::new();

    let page = pilotfy::application::resolve_and_parse_timeline_page(
        &binding, &resolver, &parser, None, 20,
    )
    .await
    .unwrap();

    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].kind, "user_message");
}
