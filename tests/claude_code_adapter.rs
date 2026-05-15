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
use llmparty::{
    adapters::GenericTestAdapter,
    application::AppState,
    storage::sqlite::{connect_sqlite, run_migrations},
    transport::http,
};
use serde_json::{Value, json};
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
        std::env::set_var("LLMPARTY_DATA_DIR", data_dir);
        std::env::set_var(
            "LLMPARTY_INTERNAL_EVENT_URL",
            "http://127.0.0.1:9/internal/v1/events",
        );
        std::env::set_var(
            "LLMPARTY_EXTERNAL_API_URL",
            "http://127.0.0.1:9/external/v1",
        );
        std::env::set_var("LLMPARTY_EXTERNAL_API_TOKEN", TOKEN);
    }
}

async fn test_state(name: &str) -> AppState {
    configure_test_runtime_env();
    GenericTestAdapter::clear_recorded_inputs();
    let dir = tempfile::tempdir().expect("tempdir");
    unsafe {
        std::env::set_var(
            "LLMPARTY_CLAUDE_TUI_COMMAND",
            "cat >> \"$LLMPARTY_WORKSPACE/claude-tui-input.log\"",
        );
        // Test-only override consumed by src/runtime/claude_code.rs so these
        // integration tests do not patch the developer's real ~/.claude.json.
        std::env::set_var(
            "LLMPARTY_CLAUDE_CONFIG_PATH",
            dir.path().join("claude.json"),
        );
    }
    let db_path = dir.path().join(format!("{name}.db"));
    let _kept_dir = dir.keep();
    let database_url = format!("sqlite://{}", db_path.display());
    let db = connect_sqlite(&database_url).await.expect("connect");
    run_migrations(&db).await.expect("migrate");
    AppState {
        db,
        external_api_token: Some(TOKEN.to_string()),
        graph: Default::default(),
        workspace_browser: Default::default(),
        dashboard: llmparty::transport::http::dashboard::ResolvedDashboard::local_default(),
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
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    (status, serde_json::from_slice(&bytes).expect("json body"))
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
    assert_eq!(session["state"], "starting");
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
    let ready = json!({
        "event_id":"evt_claude_ready",
        "session_id":session_id,
        "turn_id":null,
        "source":"agent_client",
        "client_type":"claude_code",
        "type":"session.ready",
        "time":"2026-05-08T12:00:00Z",
        "seq":1,
        "payload":{"runtime_instance_id":metadata["runtime_instance_id"]}
    });
    let (ready_status, ready_body) = post_internal_event(state.clone(), ready).await;
    assert_eq!(ready_status, StatusCode::OK, "{ready_body:?}");

    let (status, body) = request_json(
        state.clone(),
        "POST",
        &format!("/external/v1/sessions/{session_id}/inbox/messages"),
        Some(json!({"input":"hello claude"})),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED, "{body:?}");
    let turn_id = body["data"]["inbox_message"]["turn_id"]
        .as_str()
        .expect("turn id");
    let (turn_status, turn_body) = request_json(
        state.clone(),
        "GET",
        &format!("/external/v1/sessions/{session_id}/turns/{turn_id}"),
        None,
    )
    .await;
    assert_eq!(turn_status, StatusCode::OK);
    assert_eq!(turn_body["data"]["turn"]["state"], "running");

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
