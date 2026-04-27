use std::{fs, path::Path};

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

static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

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

async fn get_bytes(state: AppState, uri: &str) -> (StatusCode, Vec<u8>) {
    let response = http::router(state)
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(uri)
                .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
                .body(Body::empty())
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
        .to_bytes()
        .to_vec();
    (status, body)
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

fn write_fake_pi(dir: &Path, body: &str) -> std::path::PathBuf {
    let path = dir.join("fake-pi.sh");
    fs::write(
        &path,
        format!("#!/usr/bin/env bash\nset -euo pipefail\nread line\n{body}\n"),
    )
    .expect("write fake pi");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("chmod fake pi");
    }
    path
}

#[tokio::test]
#[allow(
    clippy::await_holding_lock,
    reason = "tests serialize process-wide env overrides"
)]
async fn pi_session_creation_exposes_m0_capabilities() {
    let _guard = ENV_LOCK.lock().expect("env lock");
    let temp = tempfile::tempdir().expect("tempdir");
    let fake = write_fake_pi(
        temp.path(),
        "echo '{\"type\":\"agent_end\",\"messages\":[]}'",
    );
    unsafe {
        std::env::set_var("LLMPARTY_PI_COMMAND", fake.display().to_string());
        std::env::remove_var("LLMPARTY_PI_ARGS");
    }
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
#[allow(
    clippy::await_holding_lock,
    reason = "tests serialize process-wide env overrides"
)]
async fn pi_turn_runs_through_fake_rpc_and_projects_completed_state() {
    let _guard = ENV_LOCK.lock().expect("env lock");
    let temp = tempfile::tempdir().expect("tempdir");
    let fake = write_fake_pi(
        temp.path(),
        r#"echo '{"type":"agent_start"}'
echo '{"type":"message_update","assistantMessageEvent":{"type":"text_delta","delta":"hello "}}'
echo '{"type":"message_update","assistantMessageEvent":{"type":"text_delta","delta":"from fake pi"}}'
echo '{"type":"agent_end","messages":[]}'"#,
    );
    unsafe {
        std::env::set_var("LLMPARTY_PI_COMMAND", fake.display().to_string());
        std::env::remove_var("LLMPARTY_PI_ARGS");
    }
    let state = test_state("m0_pi_turn").await;
    let (session_id, _) = create_pi_session(state.clone(), temp.path()).await;

    let (status, body) = request_json(
        state.clone(),
        "POST",
        &format!("/external/v1/sessions/{session_id}/turns"),
        Some(json!({"input":"run fake pi"})),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED, "{body:?}");
    let turn = &body["data"]["turn"];
    let turn_id = turn["turn_id"].as_str().expect("turn id");
    assert_eq!(turn["state"], "completed");
    assert_eq!(turn["output"]["summary"], "hello from fake pi");
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
    assert!(event_types.contains(&"turn.started"));
    assert!(event_types.contains(&"turn.output"));
    assert!(event_types.contains(&"turn.completed"));
    assert!(
        events_body["data"]["events"]
            .as_array()
            .unwrap()
            .iter()
            .all(|event| event["payload"].get("pi").is_none())
    );
}

#[tokio::test]
#[allow(
    clippy::await_holding_lock,
    reason = "tests serialize process-wide env overrides"
)]
async fn pi_artifact_is_registered_and_readable() {
    let _guard = ENV_LOCK.lock().expect("env lock");
    let temp = tempfile::tempdir().expect("tempdir");
    let fake = write_fake_pi(
        temp.path(),
        r#"echo '{"type":"agent_start"}'
echo '{"type":"message_update","assistantMessageEvent":{"type":"text_delta","delta":"artifact text"}}'
echo '{"type":"agent_end","messages":[]}'"#,
    );
    unsafe {
        std::env::set_var("LLMPARTY_PI_COMMAND", fake.display().to_string());
        std::env::remove_var("LLMPARTY_PI_ARGS");
    }
    let state = test_state("m0_pi_artifact").await;
    let (session_id, _) = create_pi_session(state.clone(), temp.path()).await;

    let (status, body) = request_json(
        state.clone(),
        "POST",
        &format!("/external/v1/sessions/{session_id}/turns"),
        Some(json!({"input":"write artifact"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body:?}");
    let turn = &body["data"]["turn"];
    let turn_id = turn["turn_id"].as_str().expect("turn id");
    let artifact_id = turn["output"]["artifact_ids"][0]
        .as_str()
        .expect("artifact id");

    let (metadata_status, metadata_body) = request_json(
        state.clone(),
        "GET",
        &format!("/external/v1/artifacts/{artifact_id}"),
        None,
    )
    .await;
    assert_eq!(metadata_status, StatusCode::OK);
    assert_eq!(metadata_body["data"]["artifact"]["session_id"], session_id);
    assert_eq!(metadata_body["data"]["artifact"]["turn_id"], turn_id);
    assert_eq!(
        metadata_body["data"]["artifact"]["preview"],
        "artifact text"
    );
    assert!(
        metadata_body["data"]["artifact"]["metadata"]
            .get("source_ref")
            .is_none()
    );

    let (content_status, content) = get_bytes(
        state,
        &format!("/external/v1/artifacts/{artifact_id}/content"),
    )
    .await;
    assert_eq!(content_status, StatusCode::OK);
    assert_eq!(content, b"artifact text");
}

#[tokio::test]
#[allow(
    clippy::await_holding_lock,
    reason = "tests serialize process-wide env overrides"
)]
async fn pi_rpc_failure_projects_turn_failed() {
    let _guard = ENV_LOCK.lock().expect("env lock");
    let temp = tempfile::tempdir().expect("tempdir");
    let fake = write_fake_pi(
        temp.path(),
        r#"echo '{"type":"agent_start"}'
echo 'not json'
exit 7"#,
    );
    unsafe {
        std::env::set_var("LLMPARTY_PI_COMMAND", fake.display().to_string());
        std::env::remove_var("LLMPARTY_PI_ARGS");
    }
    let state = test_state("m0_pi_failure").await;
    let (session_id, _) = create_pi_session(state.clone(), temp.path()).await;

    let (status, body) = request_json(
        state,
        "POST",
        &format!("/external/v1/sessions/{session_id}/turns"),
        Some(json!({"input":"fail fake pi"})),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED, "{body:?}");
    let turn = &body["data"]["turn"];
    assert_eq!(turn["state"], "failed");
    assert!(turn["failure"].as_str().unwrap().contains("pi rpc"));
}
