use std::{
    path::Path,
    process::{Command, Stdio},
    time::Duration,
};

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use llmparty::{
    application::{AppState, RuntimeObservationService},
    storage::sqlite::{connect_sqlite, run_migrations},
    transport::http,
};
use serde_json::{Value, json};
use sqlx::Row;
use tower::ServiceExt;

const TOKEN: &str = "test-token";

async fn test_state(name: &str) -> AppState {
    assert_tmux_available();
    unsafe {
        std::env::set_var(
            "LLMPARTY_PI_TUI_COMMAND",
            "cat >> \"$LLMPARTY_WORKSPACE/pi-tui-input.log\"",
        );
    }
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

fn assert_tmux_available() {
    let output = Command::new("tmux")
        .arg("-V")
        .output()
        .expect("run tmux -V");
    assert!(
        output.status.success(),
        "M1 runtime tests require a working real tmux binary"
    );
}

async fn request(
    state: AppState,
    method: &str,
    uri: &str,
    idempotency_key: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"));
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
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    let body = serde_json::from_slice(&bytes).expect("json body");
    (status, body)
}

async fn create_session(state: AppState, client_type: &str) -> String {
    create_session_with_body(state, json!({"client_type": client_type})).await
}

async fn create_session_with_body(state: AppState, body: Value) -> String {
    let (status, body) = request(state, "POST", "/external/v1/sessions", None, Some(body)).await;
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
        &format!("/external/v1/sessions/{session_id}/inbox/messages"),
        None,
        Some(json!({"input":"work through tmux"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    body["data"]["inbox_message"]["turn_id"]
        .as_str()
        .expect("turn id")
        .to_string()
}

async fn binding_metadata(state: &AppState, session_id: &str) -> Value {
    let row = sqlx::query("SELECT metadata FROM runtime_bindings WHERE session_id = ?")
        .bind(session_id)
        .fetch_one(&state.db)
        .await
        .expect("runtime binding");
    let metadata: String = row.try_get("metadata").expect("metadata");
    serde_json::from_str(&metadata).expect("metadata json")
}

fn tmux_has_session(tmux_session: &str) -> bool {
    Command::new("tmux")
        .args(["has-session", "-t", tmux_session])
        .stderr(Stdio::null())
        .status()
        .expect("tmux has-session")
        .success()
}

fn cleanup_tmux(tmux_session: &str) {
    let _ = Command::new("tmux")
        .args(["kill-session", "-t", tmux_session])
        .stderr(Stdio::null())
        .status();
}

#[tokio::test]
async fn create_generic_session_creates_real_tmux_runtime() {
    let state = test_state("m1_create_generic").await;
    let session_id = create_session(state.clone(), "generic").await;
    let metadata = binding_metadata(&state, &session_id).await;
    let tmux_session = metadata["tmux_session"].as_str().expect("tmux session");

    assert_eq!(metadata["backend"], "tmux");
    assert_eq!(metadata["restart_count"], 0);
    assert!(
        metadata["workspace"]
            .as_str()
            .is_some_and(|value| !value.is_empty())
    );
    let log_path = metadata["log_path"].as_str().expect("log path");
    assert!(!log_path.is_empty());
    assert!(Path::new(log_path).exists());
    assert!(
        metadata["started_at"]
            .as_str()
            .is_some_and(|value| !value.is_empty())
    );
    assert!(tmux_has_session(tmux_session));

    cleanup_tmux(tmux_session);
}

#[tokio::test]
async fn tmux_runtime_name_includes_handle_role_and_short_session_id() {
    let state = test_state("m1_named_tmux_runtime").await;
    let workspace = tempfile::tempdir().expect("workspace");
    let session_id = create_session_with_body(
        state.clone(),
        json!({
            "client_type": "generic",
            "workspace": workspace.path().display().to_string(),
            "handle": "@planner",
            "role": "execution reviewer"
        }),
    )
    .await;
    let metadata = binding_metadata(&state, &session_id).await;
    let tmux_session = metadata["tmux_session"].as_str().expect("tmux session");
    let id_body = session_id.rsplit('_').next().unwrap_or(&session_id);
    let short_id = id_body[id_body.len() - 8..].to_string();

    assert_eq!(metadata["handle"], "@planner");
    assert_eq!(metadata["role"], "execution reviewer");
    assert_eq!(
        tmux_session,
        format!("llmparty_planner_execution_reviewer_{short_id}")
    );
    assert!(tmux_has_session(tmux_session));

    cleanup_tmux(tmux_session);
}

#[tokio::test]
async fn terminate_session_kills_real_tmux_runtime() {
    let state = test_state("m1_terminate").await;
    let session_id = create_session(state.clone(), "generic").await;
    let metadata = binding_metadata(&state, &session_id).await;
    let tmux_session = metadata["tmux_session"]
        .as_str()
        .expect("tmux session")
        .to_string();
    assert!(tmux_has_session(&tmux_session));

    let (status, body) = request(
        state,
        "DELETE",
        &format!("/external/v1/sessions/{session_id}"),
        None,
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["session"]["state"], "exited");
    assert!(!tmux_has_session(&tmux_session));
}

#[tokio::test]
async fn restart_replaces_tmux_runtime_and_returns_idle() {
    let state = test_state("m1_restart").await;
    let session_id = create_session(state.clone(), "generic").await;
    let first = binding_metadata(&state, &session_id).await;
    let tmux_session = first["tmux_session"]
        .as_str()
        .expect("tmux session")
        .to_string();
    assert!(tmux_has_session(&tmux_session));
    tokio::time::sleep(Duration::from_millis(20)).await;

    let (status, body) = request(
        state.clone(),
        "POST",
        &format!("/external/v1/sessions/{session_id}/restart"),
        None,
        None,
    )
    .await;
    let second = binding_metadata(&state, &session_id).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["session"]["state"], "idle");
    assert_eq!(second["tmux_session"], tmux_session);
    assert_eq!(second["restart_count"], 1);
    assert_ne!(first["started_at"], second["started_at"]);
    assert!(tmux_has_session(&tmux_session));

    cleanup_tmux(&tmux_session);
}

#[tokio::test]
async fn observe_missing_tmux_runtime_projects_session_error() {
    let state = test_state("m1_observe_session_error").await;
    let session_id = create_session(state.clone(), "generic").await;
    let metadata = binding_metadata(&state, &session_id).await;
    let tmux_session = metadata["tmux_session"]
        .as_str()
        .expect("tmux session")
        .to_string();
    cleanup_tmux(&tmux_session);

    RuntimeObservationService::new(state.db.clone())
        .observe_session(&session_id)
        .await
        .expect("observe runtime");

    let (session_status, session_body) = request(
        state.clone(),
        "GET",
        &format!("/external/v1/sessions/{session_id}"),
        None,
        None,
    )
    .await;
    assert_eq!(session_status, StatusCode::OK);
    assert_eq!(session_body["data"]["session"]["state"], "error");

    let (events_status, events_body) = request(
        state,
        "GET",
        &format!("/external/v1/sessions/{session_id}/events"),
        None,
        None,
    )
    .await;
    assert_eq!(events_status, StatusCode::OK);
    let events = events_body["data"]["events"].as_array().expect("events");
    assert!(
        events.iter().any(|event| {
            event["source"] == "runtime_manager" && event["type"] == "session.error"
        })
    );
}

#[tokio::test]
async fn observe_missing_tmux_runtime_fails_active_turn() {
    let state = test_state("m1_observe_active_turn").await;
    let session_id = create_session(state.clone(), "generic").await;
    let turn_id = submit_turn(state.clone(), &session_id).await;
    let metadata = binding_metadata(&state, &session_id).await;
    let tmux_session = metadata["tmux_session"]
        .as_str()
        .expect("tmux session")
        .to_string();
    cleanup_tmux(&tmux_session);

    RuntimeObservationService::new(state.db.clone())
        .observe_session(&session_id)
        .await
        .expect("observe runtime");

    let (session_status, session_body) = request(
        state.clone(),
        "GET",
        &format!("/external/v1/sessions/{session_id}"),
        None,
        None,
    )
    .await;
    assert_eq!(session_status, StatusCode::OK);
    assert_eq!(session_body["data"]["session"]["state"], "error");

    let (turn_status, turn_body) = request(
        state.clone(),
        "GET",
        &format!("/external/v1/sessions/{session_id}/turns/{turn_id}"),
        None,
        None,
    )
    .await;
    assert_eq!(turn_status, StatusCode::OK);
    assert_eq!(turn_body["data"]["turn"]["state"], "failed");

    let (events_status, events_body) = request(
        state,
        "GET",
        &format!("/external/v1/sessions/{session_id}/events"),
        None,
        None,
    )
    .await;
    assert_eq!(events_status, StatusCode::OK);
    let events = events_body["data"]["events"].as_array().expect("events");
    assert!(events.iter().any(|event| event["type"] == "turn.failed"));
    assert!(events.iter().any(|event| event["type"] == "session.error"));
}

#[tokio::test]
async fn create_pi_session_creates_real_tmux_runtime() {
    let state = test_state("m1_create_pi").await;
    let session_id = create_session(state.clone(), "pi").await;
    let metadata = binding_metadata(&state, &session_id).await;
    let tmux_session = metadata["tmux_session"]
        .as_str()
        .expect("tmux session")
        .to_string();

    assert_eq!(metadata["backend"], "tmux");
    assert!(tmux_has_session(&tmux_session));

    let (status, body) = request(
        state,
        "GET",
        &format!("/external/v1/sessions/{session_id}"),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let capabilities = &body["data"]["session"]["capabilities"];
    assert_eq!(capabilities["accept_task"], true);
    assert_eq!(capabilities["report_turn_started"], true);
    assert_eq!(capabilities["report_turn_finished"], true);
    assert_eq!(capabilities["stream_output"], true);
    assert_eq!(capabilities["artifact_sources"], true);
    assert_eq!(capabilities["interrupt"], true);
    assert_eq!(capabilities["heartbeat"], false);

    cleanup_tmux(&tmux_session);
}
