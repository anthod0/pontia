use crate::common::test_app::TestApp;
use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
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
use serde_json::json;
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
    let ingestion = EventIngestService::for_projection_tests(state.db())
        .with_clients(crate::common::clients::clients());
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
        .ingest_reported_event(ReportedEvent::new(
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

async fn stream_response(
    state: AppState,
    authorized: bool,
    session_id: &str,
) -> axum::response::Response {
    let mut request = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/sessions/{session_id}/live-output/stream"));
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
