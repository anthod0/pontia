use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use llmparty::{
    application::AppState,
    storage::sqlite::{connect_sqlite, run_migrations},
    transport::http,
};
use serde_json::{Value, json};
use tower::ServiceExt;

const TOKEN: &str = "test-token";

async fn test_state() -> AppState {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("m6.db");
    let _kept_dir = dir.keep();
    let database_url = format!("sqlite://{}", db_path.display());
    let db = connect_sqlite(&database_url).await.expect("connect");
    run_migrations(&db).await.expect("migrate");
    AppState {
        db,
        external_api_token: Some(TOKEN.to_string()),
    }
}

async fn request(
    state: AppState,
    method: &str,
    uri: &str,
    token: Option<&str>,
    idempotency_key: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(token) = token {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    if let Some(key) = idempotency_key {
        builder = builder.header("Idempotency-Key", key);
    }
    let body = if let Some(body) = body {
        builder = builder.header(header::CONTENT_TYPE, "application/json");
        Body::from(body.to_string())
    } else {
        Body::empty()
    };

    let response = http::router(state)
        .oneshot(builder.body(body).expect("request"))
        .await
        .expect("response");

    let status = response.status();
    let body = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    let json = serde_json::from_slice(&body).expect("json body");
    (status, json)
}

async fn create_session(state: AppState) -> String {
    let (status, body) = request(
        state,
        "POST",
        "/external/v1/sessions",
        Some(TOKEN),
        None,
        Some(json!({"client_type":"generic"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    body["data"]["session"]["session_id"]
        .as_str()
        .expect("session id")
        .to_string()
}

async fn submit_turn(state: AppState, session_id: &str) -> String {
    let (status, body) = request(
        state,
        "POST",
        &format!("/external/v1/sessions/{session_id}/turns"),
        Some(TOKEN),
        None,
        Some(json!({"input":"work"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    body["data"]["turn"]["turn_id"]
        .as_str()
        .expect("turn id")
        .to_string()
}

#[tokio::test]
async fn interrupt_current_turn_returns_capability_unavailable_for_generic_runtime() {
    let state = test_state().await;
    let session_id = create_session(state.clone()).await;
    let turn_id = submit_turn(state.clone(), &session_id).await;

    let (status, body) = request(
        state.clone(),
        "POST",
        &format!("/external/v1/sessions/{session_id}/interrupt"),
        Some(TOKEN),
        None,
        None,
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "capability_unavailable");

    let (events_status, events_body) = request(
        state,
        "GET",
        &format!("/external/v1/sessions/{session_id}/turns/{turn_id}/events"),
        Some(TOKEN),
        None,
        None,
    )
    .await;
    assert_eq!(events_status, StatusCode::OK);
    let types: Vec<&str> = events_body["data"]["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|event| event["type"].as_str().unwrap())
        .collect();
    assert_eq!(types, vec!["turn.created", "turn.queued"]);
}

#[tokio::test]
async fn interrupt_specified_turn_returns_capability_unavailable_for_generic_runtime() {
    let state = test_state().await;
    let session_id = create_session(state.clone()).await;
    let turn_id = submit_turn(state.clone(), &session_id).await;

    let (status, body) = request(
        state,
        "POST",
        &format!("/external/v1/sessions/{session_id}/turns/{turn_id}/interrupt"),
        Some(TOKEN),
        Some("interrupt-once"),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "capability_unavailable");
}

#[tokio::test]
async fn terminate_session_emits_terminal_state_and_is_idempotent() {
    let state = test_state().await;
    let session_id = create_session(state.clone()).await;

    let first = request(
        state.clone(),
        "DELETE",
        &format!("/external/v1/sessions/{session_id}"),
        Some(TOKEN),
        Some("terminate-once"),
        None,
    )
    .await;
    let second = request(
        state.clone(),
        "DELETE",
        &format!("/external/v1/sessions/{session_id}"),
        Some(TOKEN),
        Some("terminate-once"),
        None,
    )
    .await;

    assert_eq!(first.0, StatusCode::OK);
    assert_eq!(second.0, StatusCode::OK);
    assert_eq!(second.1["data"], first.1["data"]);
    assert_eq!(first.1["data"]["session"]["state"], "exited");
    assert_eq!(first.1["data"]["session"]["current_turn_id"], Value::Null);

    let (events_status, events_body) = request(
        state,
        "GET",
        &format!("/external/v1/sessions/{session_id}/events"),
        Some(TOKEN),
        None,
        None,
    )
    .await;
    assert_eq!(events_status, StatusCode::OK);
    let exited_count = events_body["data"]["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|event| event["type"] == "session.exited")
        .count();
    assert_eq!(exited_count, 1);
}

#[tokio::test]
async fn restart_non_terminal_session_runs_new_start_cycle_and_is_idempotent() {
    let state = test_state().await;
    let session_id = create_session(state.clone()).await;

    let first = request(
        state.clone(),
        "POST",
        &format!("/external/v1/sessions/{session_id}/restart"),
        Some(TOKEN),
        Some("restart-once"),
        None,
    )
    .await;
    let second = request(
        state.clone(),
        "POST",
        &format!("/external/v1/sessions/{session_id}/restart"),
        Some(TOKEN),
        Some("restart-once"),
        None,
    )
    .await;

    assert_eq!(first.0, StatusCode::OK);
    assert_eq!(second.0, StatusCode::OK);
    assert_eq!(second.1["data"], first.1["data"]);
    assert_eq!(first.1["data"]["session"]["state"], "idle");
    assert_eq!(
        first.1["data"]["session"]["capabilities"]["interrupt"],
        false
    );

    let (events_status, events_body) = request(
        state,
        "GET",
        &format!("/external/v1/sessions/{session_id}/events"),
        Some(TOKEN),
        None,
        None,
    )
    .await;
    assert_eq!(events_status, StatusCode::OK);
    let types: Vec<&str> = events_body["data"]["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|event| event["type"].as_str().unwrap())
        .collect();
    assert_eq!(
        types,
        vec![
            "session.created",
            "session.starting",
            "session.started",
            "session.ready",
            "session.starting",
            "session.started",
            "session.ready",
        ]
    );
}

#[tokio::test]
async fn restart_rejects_terminal_session() {
    let state = test_state().await;
    let session_id = create_session(state.clone()).await;
    let terminate = request(
        state.clone(),
        "DELETE",
        &format!("/external/v1/sessions/{session_id}"),
        Some(TOKEN),
        None,
        None,
    )
    .await;
    assert_eq!(terminate.0, StatusCode::OK);

    let (status, body) = request(
        state,
        "POST",
        &format!("/external/v1/sessions/{session_id}/restart"),
        Some(TOKEN),
        None,
        None,
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "state_conflict");
}
