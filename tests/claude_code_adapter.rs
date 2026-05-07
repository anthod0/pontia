use std::{
    path::Path,
    path::PathBuf,
    process::{Command, Stdio},
};

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
    unsafe {
        std::env::set_var(
            "LLMPARTY_CLAUDE_TUI_COMMAND",
            "cat >> \"$LLMPARTY_WORKSPACE/claude-tui-input.log\"",
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
    let status = response.status();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    (status, serde_json::from_slice(&bytes).expect("json body"))
}

async fn create_claude_session(state: AppState, workspace: &Path) -> (String, Value) {
    let (status, body) = request_json(
        state,
        "POST",
        "/external/v1/sessions",
        Some(json!({"client_type":"claude_code","workspace":workspace.display().to_string()})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body:?}");
    let session_id = body["data"]["session"]["session_id"]
        .as_str()
        .expect("session id")
        .to_string();
    (session_id, body)
}

async fn binding_metadata(state: &AppState, session_id: &str) -> Value {
    let metadata: String =
        sqlx::query_scalar("SELECT metadata FROM runtime_bindings WHERE session_id = ?")
            .bind(session_id)
            .fetch_one(&state.db)
            .await
            .expect("binding");
    serde_json::from_str(&metadata).expect("metadata json")
}

async fn cleanup_session_runtime(state: &AppState, session_id: &str) {
    let metadata = binding_metadata(state, session_id).await;
    if let Some(tmux_session) = metadata["tmux_session"].as_str() {
        let _ = Command::new("tmux")
            .args(["kill-session", "-t", tmux_session])
            .stderr(Stdio::null())
            .status();
    }
}

#[tokio::test]
async fn claude_code_session_creation_exposes_final_output_capabilities() {
    let workspace = tempfile::tempdir().expect("workspace");
    let state = test_state("claude_caps").await;

    let (session_id, body) = create_claude_session(state.clone(), workspace.path()).await;

    let session = &body["data"]["session"];
    assert_eq!(session["client_type"], "claude_code");
    let capabilities = &session["capabilities"];
    assert_eq!(capabilities["accept_task"], true);
    assert_eq!(capabilities["report_turn_started"], false);
    assert_eq!(capabilities["report_turn_finished"], true);
    assert_eq!(capabilities["stream_output"], false);
    assert_eq!(capabilities["artifact_sources"], false);
    assert_eq!(capabilities["interrupt"], false);
    assert_eq!(capabilities["heartbeat"], false);

    cleanup_session_runtime(&state, &session_id).await;
}

#[tokio::test]
async fn claude_code_turn_submit_writes_context_and_dispatches_to_tui() {
    let workspace = tempfile::tempdir().expect("workspace");
    let state = test_state("claude_dispatch").await;
    let (session_id, _) = create_claude_session(state.clone(), workspace.path()).await;
    let metadata = binding_metadata(&state, &session_id).await;

    let (status, body) = request_json(
        state.clone(),
        "POST",
        &format!("/external/v1/sessions/{session_id}/turns"),
        Some(json!({"input":"hello claude"})),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED, "{body:?}");
    let turn = &body["data"]["turn"];
    let turn_id = turn["turn_id"].as_str().expect("turn id");
    assert_eq!(turn["state"], "running");

    let context_path = PathBuf::from(
        metadata["current_turn_file"]
            .as_str()
            .expect("current_turn_file"),
    );
    let context: Value =
        serde_json::from_str(&std::fs::read_to_string(context_path).expect("context file"))
            .expect("context json");
    assert_eq!(context["session_id"], session_id);
    assert_eq!(context["turn_id"], turn_id);
    assert_eq!(context["input"], "hello claude");
    assert_eq!(context["client_type"], "claude_code");

    let runtime_script = std::fs::read_to_string(
        PathBuf::from(metadata["runtime_dir"].as_str().expect("runtime_dir")).join("runtime.sh"),
    )
    .expect("runtime script");
    assert!(runtime_script.contains("export LLMPARTY_CLAUDE_HOOK_LOG="));
    assert!(runtime_script.contains("claude-hook.log"));

    cleanup_session_runtime(&state, &session_id).await;
}
