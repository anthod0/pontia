use std::time::Duration;

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
use tokio::io::{AsyncReadExt, AsyncWriteExt};
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
        planner: Default::default(),
        graph: Default::default(),
        workspace_browser: Default::default(),
        dashboard: llmparty::transport::http::dashboard::ResolvedDashboard::local_default(),
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

async fn seed_task_event(state: &AppState) {
    sqlx::query(
        r#"INSERT INTO tasks (task_id, state, input, routing_state, metadata)
           VALUES ('task_stream_1', 'running', 'stream task', 'ready', '{}')"#,
    )
    .execute(&state.db)
    .await
    .expect("insert task");
    sqlx::query(
        r#"INSERT INTO task_events (event_id, task_id, event_type, payload)
           VALUES ('task_evt_stream_1', 'task_stream_1', 'dag.work_item_completed', '{"work_item_id":"wi_1"}')"#,
    )
    .execute(&state.db)
    .await
    .expect("insert task event");
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
async fn dashboard_event_stream_rejects_missing_or_wrong_bearer_token() {
    let state = test_state("dashboard_auth").await;

    let missing = stream_get(state.clone(), "/external/v1/dashboard/events/stream", None).await;
    let wrong = stream_get(
        state,
        "/external/v1/dashboard/events/stream",
        Some("wrong-token"),
    )
    .await;

    assert_eq!(missing.0, StatusCode::UNAUTHORIZED);
    assert!(missing.2.contains("authentication_failed"));
    assert_eq!(wrong.0, StatusCode::UNAUTHORIZED);
    assert!(wrong.2.contains("authentication_failed"));
}

#[tokio::test]
async fn dashboard_event_stream_emits_session_events() {
    let state = test_state("dashboard_session").await;
    seed_session_events(&state).await;

    let (status, content_type, body) =
        stream_get(state, "/external/v1/dashboard/events/stream", Some(TOKEN)).await;

    assert_eq!(status, StatusCode::OK);
    assert!(content_type.starts_with("text/event-stream"));
    assert!(body.contains("event: dashboard_event"));
    assert!(body.contains("id: session:"));
    assert!(body.contains(r#""kind":"session_event""#));
    assert!(body.contains(r#""event_id":"evt_stream_1""#));
    assert!(body.contains(r#""type":"session.created""#));
}

#[tokio::test]
async fn dashboard_event_stream_emits_task_events() {
    let state = test_state("dashboard_task").await;
    seed_task_event(&state).await;

    let (status, _, body) =
        stream_get(state, "/external/v1/dashboard/events/stream", Some(TOKEN)).await;

    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("event: dashboard_event"));
    assert!(body.contains(r#""kind":"task_event""#));
    assert!(body.contains(r#""event_id":"task_evt_stream_1""#));
    assert!(body.contains(r#""event_type":"dag.work_item_completed""#));
}

#[tokio::test]
async fn dashboard_event_stream_after_cursor_does_not_repeat_read_events() {
    let state = test_state("dashboard_after").await;
    seed_session_events(&state).await;
    seed_task_event(&state).await;

    let (status, _, first_body) = stream_get(
        state.clone(),
        "/external/v1/dashboard/events/stream",
        Some(TOKEN),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let cursor = first_body
        .lines()
        .filter_map(|line| line.strip_prefix("id: "))
        .next_back()
        .expect("stream cursor");

    let (status, _, second_body) = stream_get(
        state,
        &format!("/external/v1/dashboard/events/stream?after={cursor}"),
        Some(TOKEN),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert!(!second_body.contains("evt_stream_1"));
    assert!(!second_body.contains("evt_stream_2"));
    assert!(!second_body.contains("task_evt_stream_1"));
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

#[tokio::test]
async fn graceful_shutdown_returns_after_timeout_when_event_stream_client_stays_open() {
    let state = test_state("shutdown_with_stream").await;
    seed_session_events(&state).await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind listener");
    let addr = listener.local_addr().expect("local addr");
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    let server = tokio::spawn(http::serve_with_shutdown_timeout(
        listener,
        http::router(state),
        async move {
            let _ = shutdown_rx.await;
        },
        Duration::from_millis(100),
    ));

    let mut stream = tokio::net::TcpStream::connect(addr)
        .await
        .expect("connect sse client");
    stream
        .write_all(
            b"GET /external/v1/sessions/sess_stream_1/events/stream HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer test-token\r\n\r\n",
        )
        .await
        .expect("send request");

    let mut response = Vec::new();
    let mut buffer = [0_u8; 1024];
    loop {
        let read = stream.read(&mut buffer).await.expect("read response");
        assert!(read > 0, "server closed stream before headers");
        response.extend_from_slice(&buffer[..read]);
        if response.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
    }
    assert!(
        String::from_utf8_lossy(&response).starts_with("HTTP/1.1 200 OK"),
        "unexpected response: {}",
        String::from_utf8_lossy(&response)
    );

    shutdown_tx.send(()).expect("send shutdown");
    tokio::time::timeout(Duration::from_secs(1), server)
        .await
        .expect("server should stop after shutdown timeout")
        .expect("server task")
        .expect("server result");
}
