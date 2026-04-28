use std::path::Path;

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use llmparty::{
    adapters::GenericTestAdapter,
    application::AppState,
    storage::sqlite::{connect_sqlite, run_migrations},
    transport::http,
};
use serde_json::{Value, json};
use tower::ServiceExt;

const TOKEN: &str = "test-token";

async fn test_state(name: &str) -> AppState {
    GenericTestAdapter::clear_recorded_inputs();
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

async fn request_json(
    state: AppState,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"));
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
    response_json(response).await
}

async fn response_json(response: axum::response::Response) -> (StatusCode, Value) {
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

async fn create_pi_session(state: AppState, workspace: &Path) -> (String, Value) {
    let (status, body) = request_json(
        state,
        "POST",
        "/external/v1/sessions",
        Some(json!({"client_type":"pi","workspace":workspace.display().to_string()})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body:?}");
    let session_id = body["data"]["session"]["session_id"]
        .as_str()
        .expect("session id")
        .to_string();
    (session_id, body)
}

#[tokio::test]
async fn pi_session_creation_exposes_m0_capabilities() {
    let temp = tempfile::tempdir().expect("tempdir");
    let state = test_state("m0_pi_caps").await;

    let (_session_id, body) = create_pi_session(state, temp.path()).await;

    let session = &body["data"]["session"];
    assert_eq!(session["client_type"], "pi");
    assert_eq!(session["state"], "idle");
    let capabilities = &session["capabilities"];
    assert_eq!(capabilities["accept_task"], true);
    assert_eq!(capabilities["report_turn_started"], true);
    assert_eq!(capabilities["report_turn_finished"], true);
    assert_eq!(capabilities["stream_output"], true);
    assert_eq!(capabilities["artifact_sources"], true);
    assert_eq!(capabilities["interrupt"], false);
    assert_eq!(capabilities["heartbeat"], false);
}

#[tokio::test]
async fn pi_turn_submit_stays_queued_until_adapter_events_arrive() {
    let temp = tempfile::tempdir().expect("tempdir");
    let state = test_state("m0_pi_turn_queued").await;
    let (session_id, _) = create_pi_session(state.clone(), temp.path()).await;

    let (status, body) = request_json(
        state.clone(),
        "POST",
        &format!("/external/v1/sessions/{session_id}/turns"),
        Some(json!({"input":"queue pi turn"})),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED, "{body:?}");
    let turn = &body["data"]["turn"];
    let turn_id = turn["turn_id"].as_str().expect("turn id");
    assert_eq!(turn["state"], "queued");
    assert!(turn["output"]["summary"].is_null());
    assert!(
        turn["output"]["artifact_ids"]
            .as_array()
            .is_none_or(Vec::is_empty)
    );
    assert!(GenericTestAdapter::recorded_inputs().is_empty());

    let (events_status, events_body) = request_json(
        state,
        "GET",
        &format!("/external/v1/sessions/{session_id}/turns/{turn_id}/events"),
        None,
    )
    .await;
    assert_eq!(events_status, StatusCode::OK);
    let event_types: Vec<&str> = events_body["data"]["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|event| event["type"].as_str().unwrap())
        .collect();
    assert!(event_types.contains(&"turn.created"));
    assert!(event_types.contains(&"turn.queued"));
    assert!(!event_types.contains(&"turn.started"));
    assert!(!event_types.contains(&"turn.output"));
    assert!(!event_types.contains(&"turn.completed"));
}
