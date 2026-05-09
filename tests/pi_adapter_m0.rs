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
    response_json(response).await
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

async fn report_ready(state: AppState, session_id: &str) {
    let metadata = binding_metadata(&state, session_id).await;
    let ready = json!({
        "event_id": format!("evt_ready_{session_id}"),
        "session_id":session_id,
        "turn_id":null,
        "source":"agent_client",
        "client_type":"pi",
        "type":"session.ready",
        "time":"2026-05-08T12:00:00Z",
        "seq":1,
        "payload":{"runtime_instance_id":metadata["runtime_instance_id"]}
    });
    let (status, body) = post_internal_event(state, ready).await;
    assert_eq!(status, StatusCode::OK, "{body:?}");
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

async fn cleanup_session_runtime(state: &AppState, session_id: &str) {
    if let Ok(Some(metadata)) = sqlx::query_scalar::<_, String>(
        "SELECT metadata FROM runtime_bindings WHERE session_id = ?",
    )
    .bind(session_id)
    .fetch_optional(&state.db)
    .await
        && let Ok(metadata) = serde_json::from_str::<Value>(&metadata)
        && let Some(tmux_session) = metadata["tmux_session"].as_str()
    {
        let _ = Command::new("tmux")
            .args(["kill-session", "-t", tmux_session])
            .stderr(Stdio::null())
            .status();
    }
}

#[tokio::test]
async fn pi_session_creation_exposes_m0_capabilities() {
    let temp = tempfile::tempdir().expect("tempdir");
    let state = test_state("m0_pi_caps").await;

    let (session_id, body) = create_pi_session(state.clone(), temp.path()).await;

    let session = &body["data"]["session"];
    assert_eq!(session["client_type"], "pi");
    assert_eq!(session["state"], "starting");
    let capabilities = &session["capabilities"];
    assert_eq!(capabilities["accept_task"], true);
    assert_eq!(capabilities["report_turn_started"], true);
    assert_eq!(capabilities["report_turn_finished"], true);
    assert_eq!(capabilities["stream_output"], true);
    assert_eq!(capabilities["artifact_sources"], true);
    assert_eq!(capabilities["interrupt"], false);
    assert_eq!(capabilities["heartbeat"], false);

    cleanup_session_runtime(&state, &session_id).await;
}

#[tokio::test]
async fn pi_initial_task_waits_for_agent_client_ready_before_dispatch() {
    let temp = tempfile::tempdir().expect("tempdir");
    let state = test_state("m0_pi_initial_ready").await;
    let create = tokio::spawn(request_json(
        state.clone(),
        "POST",
        "/external/v1/sessions",
        Some(json!({
            "client_type":"pi",
            "workspace":temp.path().display().to_string(),
            "initial_task":{"input":"boot prompt"}
        })),
    ));

    let (session_id, metadata) = loop {
        if let Some(row) = sqlx::query_as::<_, (String, String)>(
            "SELECT session_id, metadata FROM runtime_bindings LIMIT 1",
        )
        .fetch_optional(&state.db)
        .await
        .expect("binding poll")
        {
            break (
                row.0,
                serde_json::from_str::<Value>(&row.1).expect("metadata json"),
            );
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    assert!(!temp.path().join("pi-tui-input.log").exists());

    report_ready(state.clone(), &session_id).await;

    let (status, body) = tokio::time::timeout(Duration::from_secs(3), create)
        .await
        .expect("create session should finish after ready")
        .expect("join create");
    assert_eq!(status, StatusCode::CREATED, "{body:?}");
    assert_eq!(body["data"]["session"]["state"], "busy");
    assert_eq!(body["data"]["initial_turn"]["state"], "running");
    wait_for_file_contains(&temp.path().join("pi-tui-input.log"), "boot prompt").await;

    let context_path = Path::new(metadata["current_turn_file"].as_str().unwrap());
    let context: Value = serde_json::from_str(&std::fs::read_to_string(context_path).unwrap())
        .expect("context json");
    assert_eq!(context["input"], "boot prompt");

    cleanup_session_runtime(&state, &session_id).await;
}

async fn wait_for_file_contains(path: &Path, expected: &str) {
    for _ in 0..50 {
        if let Ok(content) = std::fs::read_to_string(path)
            && content.contains(expected)
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("{} did not contain {expected:?}", path.display());
}

#[tokio::test]
async fn pi_turn_submit_dispatches_to_tui_and_starts_without_completion() {
    let temp = tempfile::tempdir().expect("tempdir");
    let state = test_state("m0_pi_turn_queued").await;
    let (session_id, _) = create_pi_session(state.clone(), temp.path()).await;
    report_ready(state.clone(), &session_id).await;

    let (status, body) = request_json(
        state.clone(),
        "POST",
        &format!("/external/v1/sessions/{session_id}/inbox/messages"),
        Some(json!({"input":"queue pi turn"})),
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
    let turn = &turn_body["data"]["turn"];
    assert_eq!(turn["state"], "running");
    assert!(turn["output"]["summary"].is_null());
    assert!(
        turn["output"]["artifact_ids"]
            .as_array()
            .is_none_or(Vec::is_empty)
    );
    assert!(GenericTestAdapter::recorded_inputs().is_empty());

    let (events_status, events_body) = request_json(
        state.clone(),
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
    assert!(event_types.contains(&"turn.started"));
    assert!(!event_types.contains(&"turn.output"));
    assert!(!event_types.contains(&"turn.completed"));

    cleanup_session_runtime(&state, &session_id).await;
}
