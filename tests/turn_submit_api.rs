use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use pilotfy::{
    application::AppState,
    storage::sqlite::{connect_sqlite, run_migrations},
    transport::http,
};
use serde_json::{Value, json};
use tower::ServiceExt;

#[path = "support/generic_client.rs"]
mod generic_client;

use generic_client::GenericClientTestScope;

const TOKEN: &str = "test-token";

async fn test_state() -> AppState {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("m5.db");
    let _kept_dir = dir.keep();
    let database_url = format!("sqlite://{}", db_path.display());
    let db = connect_sqlite(&database_url).await.expect("connect");
    run_migrations(&db).await.expect("migrate");
    AppState {
        db,
        external_api_token: Some(TOKEN.to_string()),
        graph: Default::default(),
        workspace_browser: Default::default(),
        dashboard: pilotfy::transport::http::dashboard::ResolvedDashboard::local_default(),
        shutdown: Default::default(),
    }
}

async fn request(
    state: AppState,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"));
    if body.is_some() {
        builder = builder.header(header::CONTENT_TYPE, "application/json");
    }
    let response = http::router(state)
        .oneshot(
            builder
                .body(body.map_or_else(Body::empty, |body| Body::from(body.to_string())))
                .expect("request"),
        )
        .await
        .expect("response");

    let status = response.status();
    let body = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    let json = if body.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&body).expect("json body")
    };
    (status, json)
}

async fn post_internal_event(state: AppState, body: Value) -> (StatusCode, Value) {
    let response = http::router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/internal/v1/events")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .expect("request"),
        )
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
        Some(json!({"client_type":"generic"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    body["data"]["session"]["session_id"]
        .as_str()
        .expect("session id")
        .to_string()
}

async fn submit_inbox_message(state: AppState, session_id: &str, input: &str) -> String {
    let (status, body) = request(
        state,
        "POST",
        &format!("/external/v1/sessions/{session_id}/inbox/messages"),
        Some(json!({"input": input, "metadata": {}})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body:?}");
    body["data"]["inbox_message"]["turn_id"]
        .as_str()
        .expect("turn id")
        .to_string()
}

fn event_body(event_id: &str, event_type: &str, session_id: &str, turn_id: &str) -> Value {
    json!({
        "event_id": event_id,
        "session_id": session_id,
        "turn_id": turn_id,
        "source": "agent_adapter",
        "client_type": "generic",
        "type": event_type,
        "time": "2026-04-25T12:00:00Z",
        "seq": 10,
        "payload": {}
    })
}

#[tokio::test]
async fn post_turn_external_endpoint_is_removed() {
    let _scope = GenericClientTestScope::new().await;
    let state = test_state().await;
    let session_id = create_session(state.clone()).await;

    let (status, _) = request(
        state,
        "POST",
        &format!("/external/v1/sessions/{session_id}/turns"),
        Some(json!({"input":"continue work"})),
    )
    .await;

    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn inbox_submission_still_creates_turns_and_turn_events_are_queryable() {
    let _scope = GenericClientTestScope::new().await;
    let state = test_state().await;
    let session_id = create_session(state.clone()).await;

    let turn_id = submit_inbox_message(state.clone(), &session_id, "continue work").await;

    let (turn_status, turn_body) = request(
        state.clone(),
        "GET",
        &format!("/external/v1/sessions/{session_id}/turns/{turn_id}"),
        None,
    )
    .await;
    assert_eq!(turn_status, StatusCode::OK);
    assert_eq!(
        turn_body["data"]["turn"]["input"]["summary"],
        "continue work"
    );
    assert!(
        turn_body["data"]["turn"]["metadata"]["inbox_message_id"]
            .as_str()
            .is_some()
    );

    let (events_status, events_body) = request(
        state,
        "GET",
        &format!("/external/v1/sessions/{session_id}/turns/{turn_id}/events"),
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
async fn internal_events_advance_inbox_submitted_turn_and_session_projection() {
    let _scope = GenericClientTestScope::new().await;
    let state = test_state().await;
    let session_id = create_session(state.clone()).await;
    let turn_id = submit_inbox_message(state.clone(), &session_id, "run").await;

    let (started_status, _) = post_internal_event(
        state.clone(),
        event_body("evt_m5_started", "turn.started", &session_id, &turn_id),
    )
    .await;
    assert_eq!(started_status, StatusCode::OK);
    let (busy_status, busy_body) = request(
        state.clone(),
        "GET",
        &format!("/external/v1/sessions/{session_id}"),
        None,
    )
    .await;
    assert_eq!(busy_status, StatusCode::OK);
    assert_eq!(busy_body["data"]["session"]["state"], "busy");

    let (completed_status, _) = post_internal_event(
        state.clone(),
        event_body("evt_m5_completed", "turn.completed", &session_id, &turn_id),
    )
    .await;
    assert_eq!(completed_status, StatusCode::OK);

    let (turn_status, turn_body) = request(
        state.clone(),
        "GET",
        &format!("/external/v1/sessions/{session_id}/turns/{turn_id}"),
        None,
    )
    .await;
    assert_eq!(turn_status, StatusCode::OK);
    assert_eq!(turn_body["data"]["turn"]["state"], "completed");

    let (session_status, session_body) = request(
        state,
        "GET",
        &format!("/external/v1/sessions/{session_id}"),
        None,
    )
    .await;
    assert_eq!(session_status, StatusCode::OK);
    assert_eq!(session_body["data"]["session"]["state"], "idle");
    assert_eq!(
        session_body["data"]["session"]["current_turn_id"],
        Value::Null
    );
}
