use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use llmparty::{
    application::{AppState, EventIngestService},
    domain::{DomainEvent, EventSource, EventType},
    storage::sqlite::{connect_sqlite, run_migrations},
    transport::http,
};
use serde_json::{Value, json};
use tower::ServiceExt;

const TOKEN: &str = "test-token";
const STREAM_ONCE_HEADER: &str = "x-llmparty-test-stream-once";

async fn test_state(name: &str) -> AppState {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join(format!("{name}.db"));
    let _kept_dir = dir.keep();
    let database_url = format!("sqlite://{}", db_path.display());
    let db = connect_sqlite(&database_url).await.expect("connect");
    run_migrations(&db).await.expect("migrate");
    AppState {
        db,
        external_api_token: Some(TOKEN.to_string()),
    }
}

fn event(
    event_id: &str,
    event_type: EventType,
    session_id: &str,
    turn_id: Option<&str>,
    payload: Value,
) -> DomainEvent {
    DomainEvent::new(
        event_id.to_string(),
        session_id.to_string(),
        turn_id.map(str::to_string),
        EventSource::AgentAdapter,
        "generic".to_string(),
        event_type,
        payload,
    )
}

async fn ingest(state: &AppState, event: DomainEvent) {
    EventIngestService::new(state.db.clone())
        .ingest_event(event)
        .await
        .expect("ingest event");
}

async fn seed_session_events(state: &AppState) {
    ingest(
        state,
        event(
            "evt_stream_1",
            EventType::SessionCreated,
            "sess_stream_1",
            None,
            json!({}),
        ),
    )
    .await;
    ingest(
        state,
        event(
            "evt_stream_2",
            EventType::SessionReady,
            "sess_stream_1",
            None,
            json!({}),
        ),
    )
    .await;
}

async fn stream_get(
    state: AppState,
    uri: &str,
    token: Option<&str>,
) -> (StatusCode, String, String) {
    let mut builder = Request::builder()
        .method("GET")
        .uri(uri)
        .header(STREAM_ONCE_HEADER, "true");
    if let Some(token) = token {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }

    let response = http::router(state)
        .oneshot(builder.body(Body::empty()).expect("request"))
        .await
        .expect("response");
    let status = response.status();
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    (
        status,
        content_type,
        String::from_utf8(bytes.to_vec()).expect("utf8"),
    )
}

#[tokio::test]
async fn event_stream_rejects_missing_or_wrong_bearer_token() {
    let state = test_state("auth").await;

    let missing = stream_get(
        state.clone(),
        "/external/v1/sessions/sess_stream_1/events/stream",
        None,
    )
    .await;
    let wrong = stream_get(
        state,
        "/external/v1/sessions/sess_stream_1/events/stream",
        Some("wrong-token"),
    )
    .await;

    assert_eq!(missing.0, StatusCode::UNAUTHORIZED);
    assert!(missing.2.contains("authentication_failed"));
    assert_eq!(wrong.0, StatusCode::UNAUTHORIZED);
    assert!(wrong.2.contains("authentication_failed"));
}

#[tokio::test]
async fn session_event_stream_emits_existing_events_as_sse_frames() {
    let state = test_state("session_frames").await;
    seed_session_events(&state).await;

    let (status, content_type, body) = stream_get(
        state,
        "/external/v1/sessions/sess_stream_1/events/stream",
        Some(TOKEN),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert!(content_type.starts_with("text/event-stream"));
    assert!(body.contains("id: evt_stream_1"));
    assert!(body.contains("event: domain_event"));
    assert!(body.contains(r#""event_id":"evt_stream_1""#));
    assert!(body.contains(r#""type":"session.created""#));
    assert!(body.contains("id: evt_stream_2"));
}

#[tokio::test]
async fn session_event_stream_after_cursor_resumes_with_later_events_only() {
    let state = test_state("after_cursor").await;
    seed_session_events(&state).await;

    let (status, _, body) = stream_get(
        state,
        "/external/v1/sessions/sess_stream_1/events/stream?after=evt_stream_1",
        Some(TOKEN),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert!(!body.contains("id: evt_stream_1"));
    assert!(body.contains("id: evt_stream_2"));
}

#[tokio::test]
async fn turn_event_stream_only_emits_events_for_requested_turn() {
    let state = test_state("turn_scope").await;
    seed_session_events(&state).await;
    ingest(
        &state,
        event(
            "evt_turn_1",
            EventType::TurnStarted,
            "sess_stream_1",
            Some("turn_stream_1"),
            json!({"input":{"summary":"first"}}),
        ),
    )
    .await;
    ingest(
        &state,
        event(
            "evt_turn_1_done",
            EventType::TurnCompleted,
            "sess_stream_1",
            Some("turn_stream_1"),
            json!({"output":{"summary":"done"}}),
        ),
    )
    .await;
    ingest(
        &state,
        event(
            "evt_turn_2",
            EventType::TurnStarted,
            "sess_stream_1",
            Some("turn_stream_2"),
            json!({"input":{"summary":"second"}}),
        ),
    )
    .await;

    let (status, _, body) = stream_get(
        state,
        "/external/v1/sessions/sess_stream_1/turns/turn_stream_1/events/stream",
        Some(TOKEN),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("id: evt_turn_1"));
    assert!(!body.contains("id: evt_turn_2"));
    assert!(!body.contains("id: evt_stream_1"));
}

#[tokio::test]
async fn event_stream_rejects_cursor_outside_requested_scope() {
    let state = test_state("invalid_cursor").await;
    seed_session_events(&state).await;
    ingest(
        &state,
        event(
            "evt_other_session",
            EventType::SessionCreated,
            "sess_other",
            None,
            json!({}),
        ),
    )
    .await;

    let (status, _, body) = stream_get(
        state,
        "/external/v1/sessions/sess_stream_1/events/stream?after=evt_other_session",
        Some(TOKEN),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body.contains("invalid_request"));
}
