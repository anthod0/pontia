use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::OnceLock,
};

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use pontia_agent_clients::GenericTestClient;
use pontia_application::AppState;
use pontia_http as http;
use pontia_runtime::set_runtime_bind_addr;
use serde_json::{Value, json};
use sqlx::Row;
use tower::ServiceExt;

use crate::test_app::TestApp;

const TOKEN: &str = "test-token";

async fn test_state(name: &str) -> AppState {
    assert_tmux_available();
    configure_test_runtime_env();
    set_runtime_bind_addr("127.0.0.1:9".parse().expect("test bind addr"));
    GenericTestClient::clear_recorded_inputs();
    TestApp::builder()
        .database_name(format!("{name}.db"))
        .external_api_token(Some(TOKEN.to_string()))
        .build_state()
        .await
}

fn assert_tmux_available() {
    let output = Command::new("tmux")
        .arg("-V")
        .output()
        .expect("run tmux -V");
    assert!(
        output.status.success(),
        "pi dispatch tests require a working real tmux binary"
    );
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

fn pi_env_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

fn configure_test_runtime_env() {
    static PONTIA_HOME: OnceLock<PathBuf> = OnceLock::new();
    let pontia_home = PONTIA_HOME.get_or_init(|| {
        let dir = tempfile::tempdir().expect("pontia home tempdir");
        dir.keep()
    });
    unsafe {
        std::env::set_var("PONTIA_HOME", pontia_home);
    }
}

#[tokio::test]
async fn pi_initial_task_create_session_returns_before_ready_dispatch() {
    let state = test_state("pi_initial_task_fast_response").await;
    let workspace = tempfile::tempdir().expect("workspace");
    let _guard = pi_env_lock().lock().await;
    unsafe {
        std::env::set_var(
            "PONTIA_PI_TUI_COMMAND",
            "sh -c 'cat >> \"$PONTIA_WORKSPACE/pi-tui-input.log\"' --",
        );
    }

    let response = tokio::time::timeout(
        std::time::Duration::from_millis(750),
        request_json(
            state,
            "POST",
            "/external/v1/sessions",
            Some(json!({
                "client_type": "pi",
                "workspace": workspace.path().display().to_string(),
                "initial_task": { "input": "hello before ready" }
            })),
        ),
    )
    .await
    .expect("session creation should return before waiting for session.ready");

    let (status, body) = response;
    assert_eq!(status, StatusCode::CREATED, "{body:?}");
    assert!(body["data"]["session"]["session_id"].as_str().is_some());
}

async fn create_pi_session(state: AppState, workspace: &Path) -> String {
    let _guard = pi_env_lock().lock().await;
    unsafe {
        std::env::set_var(
            "PONTIA_PI_TUI_COMMAND",
            "sh -c 'cat >> \"$PONTIA_WORKSPACE/pi-tui-input.log\"' --",
        );
    }
    let (status, body) = request_json(
        state.clone(),
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
    report_ready(state, &session_id).await;
    session_id
}

async fn report_ready(state: AppState, session_id: &str) {
    let metadata = binding_metadata(&state, session_id).await;
    let (status, body) = request_json(
        state,
        "POST",
        "/internal/v1/events",
        Some(json!({
            "event_id": format!("evt_ready_{session_id}"),
            "session_id": session_id,
            "turn_id": null,
            "source": "agent_client",
            "client_type": "pi",
            "type": "session.ready",
            "time": "2026-05-08T12:00:00Z",
            "seq": 1,
            "payload": {
                "runtime_instance_id": metadata["runtime_instance_id"],
                "client_session_key": session_id,
                "client_session_file": metadata["log_dir"].as_str().map(|dir| format!("{dir}/pi-session.jsonl")),
                "client_session_dir": metadata["log_dir"],
                "client_cwd": metadata["workspace"]
            }
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");
}

async fn binding_metadata(state: &AppState, session_id: &str) -> Value {
    let row = sqlx::query("SELECT metadata FROM runtime_bindings WHERE session_id = ?")
        .bind(session_id)
        .fetch_one(&state.db())
        .await
        .expect("runtime binding");
    let metadata: String = row.try_get("metadata").expect("metadata");
    serde_json::from_str(&metadata).expect("metadata json")
}

async fn submit_pi_turn(state: AppState, session_id: &str, input: &str) -> Value {
    let (status, body) = request_json(
        state.clone(),
        "POST",
        &format!("/external/v1/sessions/{session_id}/inbox/messages"),
        Some(json!({"input": input})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body:?}");
    body["data"]["inbox_message"].clone()
}

fn cleanup_tmux(tmux_session: &str) {
    let _ = Command::new("tmux")
        .args(["kill-session", "-t", tmux_session])
        .stderr(Stdio::null())
        .status();
}

#[tokio::test]
async fn pi_runtime_binding_does_not_expose_file_event_log() {
    let workspace = tempfile::tempdir().expect("workspace");
    let state = test_state("pi_file_event_log_removed").await;
    let session_id = create_pi_session(state.clone(), workspace.path()).await;
    let metadata = binding_metadata(&state, &session_id).await;
    let tmux_session = metadata["tmux_session"]
        .as_str()
        .expect("tmux session")
        .to_string();

    assert!(!workspace.path().join(".pontia").exists());
    let keys = metadata.as_object().expect("metadata object").keys();
    assert!(!keys.into_iter().any(|key| key.contains("adapter")));

    cleanup_tmux(&tmux_session);
}

#[tokio::test]
async fn pi_turn_dispatch_failure_projects_failed_without_started() {
    let workspace = tempfile::tempdir().expect("workspace");
    let state = test_state("pi_dispatch_failure").await;
    let session_id = create_pi_session(state.clone(), workspace.path()).await;
    let metadata = binding_metadata(&state, &session_id).await;
    let tmux_session = metadata["tmux_session"]
        .as_str()
        .expect("tmux session")
        .to_string();
    cleanup_tmux(&tmux_session);

    let inbox = submit_pi_turn(
        state.clone(),
        &session_id,
        "this dispatch cannot reach the pi tui",
    )
    .await;
    assert_eq!(inbox["state"], "failed");
    assert!(inbox["turn_id"].is_null());

    let turn_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM turns WHERE session_id = ?")
        .bind(&session_id)
        .fetch_one(&state.db())
        .await
        .expect("turn count");
    assert_eq!(
        turn_count, 0,
        "backend must not forge a failed pi turn when dispatch cannot reach the client"
    );
}

#[tokio::test]
async fn pi_dispatch_stores_pending_current_turn_without_writing_context_file() {
    let workspace = tempfile::tempdir().expect("workspace");
    let state = test_state("pi_current_turn_context").await;
    let session_id = create_pi_session(state.clone(), workspace.path()).await;
    let metadata = binding_metadata(&state, &session_id).await;
    let tmux_session = metadata["tmux_session"]
        .as_str()
        .expect("tmux session")
        .to_string();

    let inbox = submit_pi_turn(state.clone(), &session_id, "write context for hook").await;
    assert!(inbox["turn_id"].is_null());

    assert!(!workspace.path().join(".pontia").exists());
    let updated_metadata = binding_metadata(&state, &session_id).await;
    let context = &updated_metadata["pending_current_turn"];
    assert_eq!(context["session_id"], session_id);
    assert!(
        context.get("turn_id").is_none(),
        "pi plugin owns turn_id generation"
    );
    assert_eq!(context["input"], "write context for hook");
    assert_eq!(context["inbox_message_id"], inbox["message_id"]);
    assert_eq!(context["client_type"], "pi");
    assert_eq!(
        context["runtime_instance_id"],
        metadata["runtime_instance_id"]
    );
    assert_eq!(
        context["internal_event_url"],
        "http://127.0.0.1:9/internal/v1/events"
    );

    cleanup_tmux(&tmux_session);
}

#[tokio::test]
async fn pi_current_turn_claim_returns_pending_turn_once() {
    let workspace = tempfile::tempdir().expect("workspace");
    let state = test_state("pi_current_turn_claim").await;
    let session_id = create_pi_session(state.clone(), workspace.path()).await;
    let metadata = binding_metadata(&state, &session_id).await;
    let tmux_session = metadata["tmux_session"]
        .as_str()
        .expect("tmux session")
        .to_string();

    let inbox = submit_pi_turn(state.clone(), &session_id, "claim context for hook").await;
    let (status, body) = request_json(
        state.clone(),
        "POST",
        &format!("/internal/v1/sessions/{session_id}/current-turn/claim"),
        Some(json!({
            "client_type": "pi",
            "runtime_instance_id": metadata["runtime_instance_id"]
        })),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body:?}");
    let current_turn = &body["data"]["current_turn"];
    assert_eq!(current_turn["session_id"], session_id);
    assert!(current_turn.get("turn_id").is_none());
    assert_eq!(current_turn["input"], "claim context for hook");
    assert_eq!(current_turn["inbox_message_id"], inbox["message_id"]);
    assert_eq!(current_turn["client_type"], "pi");
    assert_eq!(
        current_turn["runtime_instance_id"],
        metadata["runtime_instance_id"]
    );
    assert_eq!(
        current_turn["internal_event_url"],
        "http://127.0.0.1:9/internal/v1/events"
    );

    let (second_status, second_body) = request_json(
        state,
        "POST",
        &format!("/internal/v1/sessions/{session_id}/current-turn/claim"),
        Some(json!({
            "client_type": "pi",
            "runtime_instance_id": metadata["runtime_instance_id"]
        })),
    )
    .await;
    assert_eq!(second_status, StatusCode::OK, "{second_body:?}");
    assert!(second_body["data"]["current_turn"].is_null());

    cleanup_tmux(&tmux_session);
}

#[tokio::test]
async fn pi_runtime_exports_real_hook_environment() {
    let workspace = tempfile::tempdir().expect("workspace");
    let state = test_state("pi_hook_environment").await;
    let session_id = create_pi_session(state.clone(), workspace.path()).await;
    let metadata = binding_metadata(&state, &session_id).await;
    let tmux_session = metadata["tmux_session"]
        .as_str()
        .expect("tmux session")
        .to_string();

    assert!(!workspace.path().join(".pontia").exists());
    assert!(
        !metadata["internal_event_url"]
            .as_str()
            .unwrap_or_default()
            .contains("127.0.0.1:8080")
    );
    assert_eq!(
        metadata["internal_event_url"],
        "http://127.0.0.1:9/internal/v1/events"
    );
    let runtime_instance_id = metadata["runtime_instance_id"]
        .as_str()
        .expect("runtime_instance_id");
    assert!(runtime_instance_id.starts_with("rtinst_"));
    assert_eq!(metadata["runtime_instance_id"], runtime_instance_id);
    assert!(
        metadata["pi_hook_log"]
            .as_str()
            .expect("pi_hook_log")
            .ends_with("pi-hook.log")
    );

    cleanup_tmux(&tmux_session);
}
