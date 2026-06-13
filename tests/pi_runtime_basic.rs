use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::OnceLock,
    time::Duration,
};

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use pontia::{
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
    configure_test_runtime_env();
    GenericTestAdapter::clear_recorded_inputs();
    unsafe {
        std::env::set_var(
            "PONTIA_PI_TUI_COMMAND",
            "exec python3 -c 'import os,signal,sys; signal.signal(signal.SIGINT, signal.SIG_IGN); path=os.path.join(os.environ[\"PONTIA_WORKSPACE\"], \"pi-tui-input.log\"); f=open(path, \"a\"); [(f.write(line), f.flush()) for line in sys.stdin]'",
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
        graph: Default::default(),
        workspace_browser: Default::default(),
        dashboard: pontia::transport::http::dashboard::ResolvedDashboard::local_default(),
        shutdown: Default::default(),
        volatile_events: Default::default(),
        git_refresh: Default::default(),
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
    report_ready_with_event(state, session_id, &format!("evt_ready_{session_id}")).await;
}

async fn report_ready_with_event(state: AppState, session_id: &str, event_id: &str) {
    let metadata = binding_metadata(&state, session_id).await;
    let ready = json!({
        "event_id": event_id,
        "session_id":session_id,
        "turn_id":null,
        "source":"agent_client",
        "client_type":"pi",
        "type":"session.ready",
        "time":"2026-05-08T12:00:00Z",
        "seq":1,
        "payload":{
            "runtime_instance_id":metadata["runtime_instance_id"],
            "client_session_key":session_id,
            "client_session_file":metadata["runtime_dir"].as_str().map(|dir| format!("{dir}/pi-session.jsonl")),
            "client_session_dir":metadata["runtime_dir"],
            "client_cwd":metadata["workspace"]
        }
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
    assert_eq!(capabilities["interrupt"], true);
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
async fn pi_resume_drains_message_submitted_before_ready() {
    let temp = tempfile::tempdir().expect("tempdir");
    let state = test_state("m0_pi_resume_drains_ready").await;
    let (session_id, _) = create_pi_session(state.clone(), temp.path()).await;

    let (terminate_status, terminate_body) = request_json(
        state.clone(),
        "DELETE",
        &format!("/external/v1/sessions/{session_id}"),
        None,
    )
    .await;
    assert_eq!(terminate_status, StatusCode::OK, "{terminate_body:?}");
    assert_eq!(terminate_body["data"]["session"]["state"], "exited");

    let (resume_status, resume_body) = request_json(
        state.clone(),
        "POST",
        &format!("/external/v1/sessions/{session_id}/resume"),
        None,
    )
    .await;
    assert_eq!(resume_status, StatusCode::OK, "{resume_body:?}");
    assert_eq!(resume_body["data"]["session"]["state"], "starting");

    let (queued_status, queued_body) = request_json(
        state.clone(),
        "POST",
        &format!("/external/v1/sessions/{session_id}/inbox/messages"),
        Some(json!({"input":"continue after resume"})),
    )
    .await;
    assert_eq!(queued_status, StatusCode::CREATED, "{queued_body:?}");
    let message_id = queued_body["data"]["inbox_message"]["message_id"]
        .as_str()
        .expect("message id")
        .to_string();
    assert_eq!(queued_body["data"]["inbox_message"]["state"], "pending");
    assert_eq!(queued_body["data"]["inbox_message"]["turn_id"], Value::Null);

    report_ready_with_event(state.clone(), &session_id, "evt_ready_after_resume").await;

    let (message_status, message_body) = request_json(
        state.clone(),
        "GET",
        &format!("/external/v1/sessions/{session_id}/inbox/messages/{message_id}"),
        None,
    )
    .await;
    assert_eq!(message_status, StatusCode::OK, "{message_body:?}");
    let message = &message_body["data"]["inbox_message"];
    assert_eq!(message["state"], "dispatched");
    let turn_id = message["turn_id"].as_str().expect("turn id");

    let (turn_status, turn_body) = request_json(
        state.clone(),
        "GET",
        &format!("/external/v1/sessions/{session_id}/turns/{turn_id}"),
        None,
    )
    .await;
    assert_eq!(turn_status, StatusCode::OK, "{turn_body:?}");
    assert_eq!(turn_body["data"]["turn"]["state"], "running");
    assert_eq!(
        turn_body["data"]["turn"]["input"]["summary"],
        "continue after resume"
    );

    cleanup_session_runtime(&state, &session_id).await;
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

#[tokio::test]
async fn pi_interrupt_now_interrupts_active_turn_and_dispatches_next_message() {
    let temp = tempfile::tempdir().expect("tempdir");
    let state = test_state("m0_pi_interrupt_now").await;
    let (session_id, _) = create_pi_session(state.clone(), temp.path()).await;
    report_ready(state.clone(), &session_id).await;
    tokio::time::sleep(Duration::from_millis(100)).await;

    let (first_status, first_body) = request_json(
        state.clone(),
        "POST",
        &format!("/external/v1/sessions/{session_id}/inbox/messages"),
        Some(json!({"input":"first pi turn"})),
    )
    .await;
    assert_eq!(first_status, StatusCode::CREATED, "{first_body:?}");
    let first_turn_id = first_body["data"]["inbox_message"]["turn_id"]
        .as_str()
        .expect("first turn id")
        .to_string();

    let (interrupt_status, interrupt_body) = request_json(
        state.clone(),
        "POST",
        &format!("/external/v1/sessions/{session_id}/inbox/messages"),
        Some(json!({"input":"replacement pi turn", "delivery_policy":"interrupt_now"})),
    )
    .await;
    assert_eq!(interrupt_status, StatusCode::CREATED, "{interrupt_body:?}");
    let interrupt_message = &interrupt_body["data"]["inbox_message"];
    assert_eq!(interrupt_message["state"], "dispatched");
    let second_turn_id = interrupt_message["turn_id"]
        .as_str()
        .expect("second turn id");
    assert_ne!(second_turn_id, first_turn_id);

    let (first_turn_status, first_turn_body) = request_json(
        state.clone(),
        "GET",
        &format!("/external/v1/sessions/{session_id}/turns/{first_turn_id}"),
        None,
    )
    .await;
    assert_eq!(first_turn_status, StatusCode::OK);
    assert_eq!(first_turn_body["data"]["turn"]["state"], "interrupted");

    let (second_turn_status, second_turn_body) = request_json(
        state.clone(),
        "GET",
        &format!("/external/v1/sessions/{session_id}/turns/{second_turn_id}"),
        None,
    )
    .await;
    assert_eq!(second_turn_status, StatusCode::OK);
    assert_eq!(second_turn_body["data"]["turn"]["state"], "running");

    cleanup_session_runtime(&state, &session_id).await;
}
