use crate::common::test_app::TestApp;
use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use pontia_agent_clients::raw_transcripts::{ManagedToolUse, ManagedToolUseInput};
use pontia_application::{
    AppState, EventIngestService, LiveOutputBatch, LiveOutputItem, LiveOutputSnapshotReplacement,
    LiveOutputUpdate,
};
use pontia_core::{
    domain::{EventSource, EventType, ReportedEvent},
    ids::new_event_id,
};
use pontia_http as http;
use pontia_storage_sqlite::repositories::runtime_bindings::{
    RuntimeBindingUpsertRecord, SqliteRuntimeBindingRepository,
};
use serde_json::{Value, json};
use tower::ServiceExt;

async fn state_with_running_turn() -> AppState {
    let state = TestApp::builder()
        .database_name("live-output.db")
        .external_api_token(Some("test-token".to_string()))
        .build_state()
        .await;
    seed_running_turn(&state, "sess_live", "turn_live", "rtinst_live").await;
    state
}

async fn seed_running_turn(state: &AppState, session_id: &str, turn_id: &str, runtime_id: &str) {
    let ingestion = EventIngestService::new(state.db());
    ingestion
        .ingest_reported_event(ReportedEvent::new(
            new_event_id().to_string(),
            session_id.into(),
            None,
            EventSource::ExternalApi,
            "pi".into(),
            EventType::SessionCreated,
            json!({}),
        ))
        .await
        .unwrap();
    SqliteRuntimeBindingRepository::new(state.db())
        .upsert_binding(RuntimeBindingUpsertRecord {
            session_id: session_id.into(),
            runtime_kind: "tmux".into(),
            runtime_instance_id: Some(runtime_id.into()),
            binding_state: "confirmed".into(),
            runtime_handle: None,
            start_command: None,
            launch_cwd: Some("/tmp".into()),
            internal_event_url: None,
            started_at: None,
            last_seen_at: None,
            restart_count: 0,
            tmux_socket_path: None,
            tmux_pane_id: None,
            process_fingerprint: None,
            capabilities: "{}".into(),
            diagnostics: "{}".into(),
            adapter_details: "{}".into(),
        })
        .await
        .unwrap();
    ingestion
        .ingest_confirmed_event(ReportedEvent::new(
            new_event_id().to_string(),
            session_id.into(),
            Some(turn_id.into()),
            EventSource::AgentClient,
            "pi".into(),
            EventType::TurnStarted,
            json!({"runtime_instance_id": runtime_id, "input_summary": "test"}),
        ))
        .await
        .unwrap();
}

fn producer(
    session_id: &str,
    turn_id: &str,
    stream_id: &str,
    runtime_instance_id: &str,
) -> pontia_application::LiveOutputProducer {
    pontia_application::LiveOutputProducer {
        identity: pontia_application::LiveOutputIdentity {
            session_id: session_id.into(),
            turn_id: turn_id.into(),
            stream_id: stream_id.into(),
        },
        runtime_instance_id: runtime_instance_id.into(),
    }
}

async fn post(state: AppState, path: &str, body: Value) -> (StatusCode, Value) {
    let response = http::router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(path)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}

async fn stream_response(
    state: AppState,
    authorized: bool,
    session_id: &str,
) -> axum::response::Response {
    let mut request = Request::builder().method("GET").uri(format!(
        "/external/v1/sessions/{session_id}/live-output/stream"
    ));
    if authorized {
        request = request.header(header::AUTHORIZATION, "Bearer test-token");
    }
    http::router(state)
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap()
}

async fn body_until(body: &mut Body, needle: &str) -> String {
    let mut output = String::new();
    while !output.contains(needle) {
        let frame = tokio::time::timeout(std::time::Duration::from_secs(1), body.frame())
            .await
            .expect("SSE frame timeout")
            .expect("SSE body ended")
            .expect("SSE frame");
        if let Ok(data) = frame.into_data() {
            output.push_str(std::str::from_utf8(&data).unwrap());
        }
    }
    output
}

fn identity(operation: Value) -> Value {
    let mut request = json!({
        "session_id": "sess_live",
        "turn_id": "turn_live",
        "runtime_instance_id": "rtinst_live",
        "stream_id": "stream_live"
    });
    request
        .as_object_mut()
        .unwrap()
        .extend(operation.as_object().unwrap().clone());
    request
}

#[tokio::test]
async fn external_live_output_stream_authenticates_and_checks_the_session() {
    let state = state_with_running_turn().await;

    assert_eq!(
        stream_response(state.clone(), false, "sess_live")
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        stream_response(state, true, "missing").await.status(),
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn external_live_output_stream_sends_snapshot_updates_and_closed() {
    let state = state_with_running_turn().await;
    state
        .live_output()
        .replace_snapshot(LiveOutputSnapshotReplacement {
            producer: producer("sess_live", "turn_live", "stream_live", "rtinst_live"),
            sequence: 1,
            items: vec![LiveOutputItem::AssistantText {
                item_id: "text_1".into(),
                text: "hello".into(),
            }],
        })
        .await
        .unwrap();

    let response = stream_response(state.clone(), true, "sess_live").await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()[header::CONTENT_TYPE],
        "text/event-stream"
    );
    let mut body = response.into_body();
    let snapshot = body_until(&mut body, "\n\n").await;
    assert!(snapshot.contains("event: snapshot"), "{snapshot}");
    assert!(snapshot.contains(r#""sequence":1"#), "{snapshot}");

    seed_running_turn(&state, "sess_other", "turn_other", "rtinst_other").await;
    state
        .live_output()
        .replace_snapshot(LiveOutputSnapshotReplacement {
            producer: producer("sess_other", "turn_other", "stream_other", "rtinst_other"),
            sequence: 1,
            items: Vec::new(),
        })
        .await
        .unwrap();
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(20), body.frame())
            .await
            .is_err(),
        "another Session must not produce an SSE frame"
    );

    state
        .live_output()
        .publish_batch(LiveOutputBatch {
            producer: producer("sess_live", "turn_live", "stream_live", "rtinst_live"),
            first_sequence: 2,
            updates: vec![LiveOutputUpdate::AssistantTextDelta {
                item_id: "text_1".into(),
                delta: " world".into(),
            }],
        })
        .await
        .unwrap();
    let updates = body_until(&mut body, "\n\n").await;
    assert!(updates.contains("event: updates"), "{updates}");
    assert!(updates.contains(r#""first_sequence":2"#), "{updates}");

    state.live_output().discard_turn("sess_live", "turn_live");
    let closed = body_until(&mut body, "\n\n").await;
    assert!(closed.contains("event: closed"), "{closed}");
    assert!(closed.contains(r#""sequence":2"#), "{closed}");
    assert!(closed.contains(r#""reason":"invalidated""#), "{closed}");
}

#[tokio::test]
async fn live_output_ingress_rejects_managed_input_that_contradicts_the_tool_name() {
    let state = state_with_running_turn().await;
    let (status, _) = post(
        state.clone(),
        "/internal/v1/live-output",
        identity(json!({
            "type": "snapshot",
            "sequence": 1,
            "items": [{
                "kind": "tool_call",
                "item_id": "tool_1",
                "call_id": "call_1",
                "tool_name": "read",
                "arguments": {"path": "README.md"},
                "managed_tool_use": {
                    "tool_name": "read",
                    "input": {"type": "bash", "command": "cat README.md"}
                }
            }]
        })),
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT);
    assert!(
        state
            .live_output()
            .snapshot("sess_live", "turn_live")
            .is_none()
    );
}

#[tokio::test]
async fn live_output_ingress_applies_ordered_updates_and_recovers_from_a_gap() {
    let state = state_with_running_turn().await;
    let path = "/internal/v1/live-output";

    let (status, body) = post(
        state.clone(),
        path,
        identity(json!({
            "type": "snapshot",
            "sequence": 1,
            "items": [{"kind": "assistant_text", "item_id": "text_1", "text": "hello"}]
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["accepted_sequence"], 1);

    let append = identity(json!({
        "type": "append",
        "first_sequence": 2,
        "updates": [
            {"type": "assistant_text_delta", "item_id": "text_1", "delta": " world"},
            {"type": "tool_call", "item_id": "tool_1", "call_id": "call_1", "tool_name": "read", "arguments": {"path": "README.md"}, "managed_tool_use": {"tool_name": "read", "input": {"type": "read", "path": "README.md"}}},
            {"type": "assistant_text_delta", "item_id": "text_2", "delta": "done"}
        ]
    }));
    let (status, body) = post(state.clone(), path, append.clone()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["accepted_sequence"], 4);
    assert_eq!(body["duplicate"], false);

    let (status, body) = post(state.clone(), path, append).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["duplicate"], true);

    let snapshot = state
        .live_output()
        .snapshot("sess_live", "turn_live")
        .unwrap();
    assert_eq!(snapshot.sequence, 4);
    assert_eq!(
        snapshot.items,
        vec![
            LiveOutputItem::AssistantText {
                item_id: "text_1".into(),
                text: "hello world".into(),
            },
            LiveOutputItem::ToolCall {
                item_id: "tool_1".into(),
                call_id: "call_1".into(),
                tool_name: "read".into(),
                arguments: json!({"path": "README.md"}),
                managed_tool_use: Some(ManagedToolUse {
                    tool_name: "read".into(),
                    input: ManagedToolUseInput::Read {
                        path: "README.md".into(),
                        start_line: None,
                        end_line: None,
                    },
                }),
            },
            LiveOutputItem::AssistantText {
                item_id: "text_2".into(),
                text: "done".into(),
            },
        ]
    );

    let (status, body) = post(
        state.clone(),
        path,
        identity(json!({
            "type": "append",
            "first_sequence": 6,
            "updates": [{"type": "assistant_text_delta", "item_id": "text_2", "delta": "!"}]
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["resync_required"], true);
    assert_eq!(body["accepted_sequence"], 4);

    let close = identity(json!({"type": "stream_closed", "sequence": 5}));
    let (status, body) = post(state.clone(), path, close.clone()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["accepted_sequence"], 5);
    assert!(
        state
            .live_output()
            .snapshot("sess_live", "turn_live")
            .is_none()
    );
    let (status, body) = post(state.clone(), path, close).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["duplicate"], true);

    let event_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events")
        .fetch_one(&state.db())
        .await
        .unwrap();
    assert_eq!(event_count, 2, "live output must not create durable events");
}

#[tokio::test]
async fn live_output_ingress_fences_runtime_and_terminal_facts_clear_state() {
    let state = state_with_running_turn().await;
    let path = "/internal/v1/live-output";
    let mut stale = identity(json!({
        "type": "snapshot",
        "sequence": 1,
        "items": [{"kind": "assistant_text", "item_id": "text_1", "text": "hello"}]
    }));
    stale["runtime_instance_id"] = json!("rtinst_stale");

    let (status, _) = post(state.clone(), path, stale).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(
        state
            .live_output()
            .snapshot("sess_live", "turn_live")
            .is_none()
    );

    let mut wrong_turn = identity(json!({
        "type": "snapshot",
        "sequence": 1,
        "items": []
    }));
    wrong_turn["turn_id"] = json!("turn_missing");
    let (status, _) = post(state.clone(), path, wrong_turn).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, _) = post(
        state.clone(),
        path,
        identity(json!({
            "type": "snapshot",
            "sequence": 1,
            "items": [{"kind": "assistant_text", "item_id": "text_1", "text": "hello"}]
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, _) = post(
        state.clone(),
        "/internal/v1/events",
        json!({
            "session_id": "sess_live",
            "turn_id": "turn_live",
            "type": "turn.completed",
            "data": {}
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        state
            .live_output()
            .snapshot("sess_live", "turn_live")
            .is_none()
    );
}
