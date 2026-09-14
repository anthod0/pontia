use crate::common::test_app::TestApp;
use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use pontia_application::{AppState, EventIngestService, LiveOutputItem};
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
        .external_api_token(None)
        .build_state()
        .await;
    let ingestion = EventIngestService::new(state.db());
    ingestion
        .ingest_reported_event(ReportedEvent::new(
            new_event_id().to_string(),
            "sess_live".into(),
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
            session_id: "sess_live".into(),
            runtime_kind: "tmux".into(),
            runtime_instance_id: Some("rtinst_live".into()),
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
            "sess_live".into(),
            Some("turn_live".into()),
            EventSource::AgentClient,
            "pi".into(),
            EventType::TurnStarted,
            json!({"runtime_instance_id": "rtinst_live", "input_summary": "test"}),
        ))
        .await
        .unwrap();
    state
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
            {"type": "tool_call", "item_id": "tool_1", "call_id": "call_1", "tool_name": "read", "arguments": {"path": "README.md"}},
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
