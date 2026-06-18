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
use pontia::application::AppState;
use pontia::transport::http;
use pontia_storage_sqlite::{connect_sqlite, run_migrations};
use serde_json::{Value, json};
use sqlx::Row;
use tower::ServiceExt;

const TOKEN: &str = "test-token";

fn configure_test_runtime_env() {
    static DATA_DIR: OnceLock<PathBuf> = OnceLock::new();
    let data_dir = DATA_DIR.get_or_init(|| {
        let dir = tempfile::tempdir().expect("runtime data tempdir");
        let path = dir.path().join("data");
        let _kept_dir = dir.keep();
        path
    });
    unsafe {
        std::env::set_var("PONTIA_DATA_DIR", data_dir);
        std::env::set_var(
            "PONTIA_INTERNAL_EVENT_URL",
            "http://127.0.0.1:9/internal/v1/events",
        );
        std::env::set_var("PONTIA_EXTERNAL_API_URL", "http://127.0.0.1:9/external/v1");
        std::env::set_var("PONTIA_EXTERNAL_API_TOKEN", TOKEN);
    }
}

async fn test_state(name: &str) -> AppState {
    assert_tmux_available();
    configure_test_runtime_env();
    unsafe {
        std::env::set_var(
            "PONTIA_PI_TUI_COMMAND",
            "cat >> \"$PONTIA_WORKSPACE/pi-tui-input.log\"",
        );
    }
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join(format!("{name}.db"));
    let _kept_dir = dir.keep();
    let database_url = format!("sqlite://{}", db_path.display());
    let db = connect_sqlite(&database_url).await.expect("connect");
    run_migrations(&db).await.expect("migrate");
    AppState::builder(db)
        .external_api_token(Some(TOKEN.to_string()))
        .build()
}

fn assert_tmux_available() {
    let output = Command::new("tmux")
        .arg("-V")
        .output()
        .expect("run tmux -V");
    assert!(
        output.status.success(),
        "tmux runtime smoke tests require a working real tmux binary"
    );
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
    let (status, body) = request(
        state,
        "POST",
        "/external/v1/sessions",
        Some(json!({"client_type": client_type})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body:?}");
    body["data"]["session"]["session_id"]
        .as_str()
        .expect("session id")
        .to_string()
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

async fn binding_structured_fields(
    state: &AppState,
    session_id: &str,
) -> (Option<String>, Option<String>, Option<String>) {
    let row = sqlx::query(
        "SELECT runtime_instance_id, launch_cwd, last_seen_at FROM runtime_bindings WHERE session_id = ?",
    )
    .bind(session_id)
    .fetch_one(&state.db())
    .await
    .expect("runtime binding");
    (
        row.try_get("runtime_instance_id")
            .expect("runtime_instance_id"),
        row.try_get("launch_cwd").expect("launch_cwd"),
        row.try_get("last_seen_at").expect("last_seen_at"),
    )
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
async fn create_tmux_client_session_creates_real_tmux_runtime() {
    let state = test_state("m1_create_tmux_client").await;
    let session_id = create_session(state.clone(), "pi").await;
    let metadata = binding_metadata(&state, &session_id).await;
    let tmux_session = metadata["tmux_session"].as_str().expect("tmux session");

    let (runtime_instance_id, launch_cwd, last_seen_at) =
        binding_structured_fields(&state, &session_id).await;

    assert_eq!(metadata["backend"], "tmux");
    assert_eq!(metadata["restart_count"], 0);
    assert_eq!(
        runtime_instance_id.as_deref(),
        metadata["runtime_instance_id"].as_str()
    );
    assert_eq!(launch_cwd.as_deref(), metadata["workspace"].as_str());
    assert!(
        last_seen_at
            .as_deref()
            .is_some_and(|value| !value.is_empty())
    );
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
async fn create_pi_session_exposes_pi_capabilities() {
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
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");
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
