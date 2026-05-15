use std::{fs, path::Path};

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use llmparty::{
    adapters::{AdapterCapabilities, ArtifactRegistration, GenericTestAdapter},
    application::{AppState, ArtifactRegistrationService},
    storage::sqlite::{connect_sqlite, run_migrations},
    transport::http,
};
use serde_json::{Value, json};
use tower::ServiceExt;

#[path = "support/generic_client.rs"]
mod generic_client;

use generic_client::GenericClientTestScope;

const TOKEN: &str = "test-token";

async fn test_state(name: &str) -> AppState {
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
        dashboard: llmparty::transport::http::dashboard::ResolvedDashboard::local_default(),
    }
}

async fn post_json(
    state: AppState,
    uri: &str,
    token: Option<&str>,
    body: Value,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method("POST")
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(token) = token {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }

    let response = http::router(state)
        .oneshot(builder.body(Body::from(body.to_string())).expect("request"))
        .await
        .expect("response");
    response_json(response).await
}

async fn get_json(state: AppState, uri: &str) -> (StatusCode, Value) {
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

async fn create_session(state: AppState) -> String {
    let (status, body) = post_json(
        state,
        "/external/v1/sessions",
        Some(TOKEN),
        json!({"client_type":"generic"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    body["data"]["session"]["session_id"]
        .as_str()
        .expect("session id")
        .to_string()
}

async fn create_session_with_body(state: AppState, body: Value) -> Value {
    let (status, body) = post_json(state, "/external/v1/sessions", Some(TOKEN), body).await;
    assert_eq!(status, StatusCode::CREATED, "{body:?}");
    body
}

async fn submit_turn(state: AppState, session_id: &str, input: &str) -> (String, Value) {
    let (status, body) = post_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/inbox/messages"),
        Some(TOKEN),
        json!({"input":input,"turn_id":"turn_client_must_be_ignored"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let turn_id = body["data"]["inbox_message"]["turn_id"]
        .as_str()
        .unwrap()
        .to_string();
    let (turn_status, turn_body) = get_json(
        state,
        &format!("/external/v1/sessions/{session_id}/turns/{turn_id}"),
    )
    .await;
    assert_eq!(turn_status, StatusCode::OK);
    (
        turn_id,
        json!({ "data": { "turn": turn_body["data"]["turn"].clone() } }),
    )
}

fn file_url(path: &Path) -> String {
    format!("file://{}", path.display())
}

#[tokio::test]
async fn generic_test_adapter_can_expose_pi_like_capabilities_without_pi_runtime() {
    let _scope = GenericClientTestScope::new()
        .await
        .with_capabilities(AdapterCapabilities::pi_m0_default());
    let state = test_state("m8_pi_like_capabilities").await;
    let session_id = create_session(state.clone()).await;

    let (status, body) = get_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}"),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let session = &body["data"]["session"];
    assert_eq!(session["client_type"], "generic");
    assert_eq!(session["capabilities"]["interrupt"], true);
    assert_eq!(session["capabilities"]["stream_output"], true);
    assert_eq!(session["capabilities"]["artifact_sources"], true);

    let metadata: String =
        sqlx::query_scalar("SELECT metadata FROM runtime_bindings WHERE session_id = ?")
            .bind(&session_id)
            .fetch_one(&state.db)
            .await
            .expect("runtime metadata");
    let metadata: Value = serde_json::from_str(&metadata).expect("metadata json");
    assert_eq!(metadata["backend"], "in_process_test");
    assert!(metadata.get("tmux_session").is_none());
}

#[tokio::test]
async fn capability_model_declares_all_mvp_adapter_capabilities() {
    let _scope = GenericClientTestScope::new().await;
    let state = test_state("m8_capabilities").await;
    let session_id = create_session(state.clone()).await;

    let (status, body) = get_json(state, &format!("/external/v1/sessions/{session_id}")).await;

    assert_eq!(status, StatusCode::OK);
    let capabilities = &body["data"]["session"]["capabilities"];
    assert_eq!(capabilities["accept_task"], true);
    assert_eq!(capabilities["report_turn_started"], true);
    assert_eq!(capabilities["report_turn_finished"], true);
    assert_eq!(capabilities["interrupt"], false);
    assert_eq!(capabilities["stream_output"], false);
    assert_eq!(capabilities["heartbeat"], false);
    assert_eq!(capabilities["artifact_sources"], false);

    assert!(AdapterCapabilities::generic_default().accept_task);
}

#[tokio::test]
async fn generic_initial_task_dispatches_in_process() {
    let _scope = GenericClientTestScope::new().await;
    let state = test_state("m8_in_process_initial_task").await;
    let _scope = _scope.auto_start_turn().write_current_turn_context();

    let body = create_session_with_body(
        state.clone(),
        json!({"client_type":"generic","initial_task":{"input":"boot generic"}}),
    )
    .await;

    let session_id = body["data"]["session"]["session_id"].as_str().unwrap();
    let turn = &body["data"]["initial_turn"];
    let turn_id = turn["turn_id"].as_str().unwrap();
    assert_eq!(body["data"]["session"]["state"], "busy");
    assert_eq!(turn["state"], "running");
    assert!(GenericTestAdapter::recorded_inputs().iter().any(|input| {
        input.session_id == session_id && input.turn_id == turn_id && input.input == "boot generic"
    }));
}

#[tokio::test]
async fn generic_dispatch_starts_turn_and_writes_current_turn_context_in_process() {
    let _scope = GenericClientTestScope::new().await;
    let state = test_state("m8_in_process_dispatch").await;
    let _scope = _scope.auto_start_turn().write_current_turn_context();
    let session_id = create_session(state.clone()).await;

    let (turn_id, body) = submit_turn(state.clone(), &session_id, "in process task").await;

    assert_eq!(body["data"]["turn"]["state"], "running");
    let metadata: String =
        sqlx::query_scalar("SELECT metadata FROM runtime_bindings WHERE session_id = ?")
            .bind(&session_id)
            .fetch_one(&state.db)
            .await
            .expect("runtime metadata");
    let metadata: Value = serde_json::from_str(&metadata).expect("metadata json");
    let context_path = metadata["current_turn_file"]
        .as_str()
        .expect("context path");
    let context: Value =
        serde_json::from_str(&fs::read_to_string(context_path).expect("current turn context"))
            .expect("context json");
    assert_eq!(context["session_id"], session_id);
    assert_eq!(context["turn_id"], turn_id);
    assert_eq!(context["input"], "in process task");
    assert_eq!(context["client_type"], "generic");
}

#[tokio::test]
async fn turn_input_handoff_uses_control_plane_assigned_identity() {
    let _scope = GenericClientTestScope::new().await;
    let state = test_state("m8_turn_input").await;
    let session_id = create_session(state.clone()).await;

    let (turn_id, body) = submit_turn(state, &session_id, "adapter contract task").await;

    assert_ne!(turn_id, "turn_client_must_be_ignored");
    assert!(turn_id.starts_with("turn_"));
    assert_eq!(body["data"]["turn"]["session_id"], session_id);

    let inputs = GenericTestAdapter::recorded_inputs();
    assert!(inputs.iter().any(|input| {
        input.session_id == session_id
            && input.turn_id == turn_id
            && input.input == "adapter contract task"
    }));
}

#[tokio::test]
async fn event_source_returns_turn_facts_through_internal_event_api() {
    let _scope = GenericClientTestScope::new().await;
    let state = test_state("m8_event_source").await;
    let session_id = create_session(state.clone()).await;
    let (turn_id, _) = submit_turn(state.clone(), &session_id, "run to completion").await;

    for (idx, event_type, payload) in [
        (1, "turn.started", json!({})),
        (2, "turn.output", json!({"output":{"summary":"working"}})),
        (3, "turn.completed", json!({"output":{"summary":"done"}})),
    ] {
        let (status, _body) = post_json(
            state.clone(),
            "/internal/v1/events",
            None,
            json!({
                "event_id": format!("evt_m8_return_{idx}"),
                "session_id": session_id,
                "turn_id": turn_id,
                "source": "agent_adapter",
                "client_type": "generic",
                "type": event_type,
                "time": "2026-04-25T12:00:00Z",
                "seq": idx + 10,
                "payload": payload
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }

    let (turn_status, turn_body) = get_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/turns/{turn_id}"),
    )
    .await;
    assert_eq!(turn_status, StatusCode::OK);
    assert_eq!(turn_body["data"]["turn"]["state"], "completed");
    assert_eq!(turn_body["data"]["turn"]["output"]["summary"], "working");

    let (events_status, events_body) = get_json(
        state,
        &format!("/external/v1/sessions/{session_id}/turns/{turn_id}/events"),
    )
    .await;
    assert_eq!(events_status, StatusCode::OK);
    let returned = events_body["data"]["events"].as_array().unwrap();
    assert!(
        returned
            .iter()
            .any(|event| event["source"] == "agent_adapter")
    );
    assert!(
        returned
            .iter()
            .all(|event| event["payload"].get("pi").is_none())
    );
    assert!(
        returned
            .iter()
            .all(|event| event["payload"].get("codex").is_none())
    );
}

#[tokio::test]
async fn artifact_source_provider_registers_readable_artifacts_without_exposing_source_ref() {
    let _scope = GenericClientTestScope::new().await;
    let state = test_state("m8_artifact_source").await;
    let session_id = create_session(state.clone()).await;
    let (turn_id, _) = submit_turn(state.clone(), &session_id, "produce artifact").await;
    let dir = tempfile::tempdir().expect("artifact dir");
    let artifact_path = dir.path().join("result.txt");
    fs::write(&artifact_path, "artifact from adapter").expect("write artifact");

    let registration = ArtifactRegistration {
        artifact_id: "art_m8_registered".to_string(),
        session_id: session_id.clone(),
        turn_id: Some(turn_id.clone()),
        kind: "file".to_string(),
        name: "result.txt".to_string(),
        source_ref: file_url(&artifact_path),
        size_bytes: Some(21),
        metadata: json!({"preview":"artifact from adapter","source_ref":"must stay internal"}),
    };
    ArtifactRegistrationService::new(state.db.clone())
        .register(registration)
        .await
        .expect("register artifact");

    let (metadata_status, metadata_body) =
        get_json(state.clone(), "/external/v1/artifacts/art_m8_registered").await;
    assert_eq!(metadata_status, StatusCode::OK);
    assert_eq!(metadata_body["data"]["artifact"]["session_id"], session_id);
    assert_eq!(metadata_body["data"]["artifact"]["turn_id"], turn_id);
    assert_eq!(
        metadata_body["data"]["artifact"]["preview"],
        "artifact from adapter"
    );
    assert!(
        metadata_body["data"]["artifact"]["metadata"]
            .get("source_ref")
            .is_none()
    );

    let (content_status, content) =
        get_bytes(state, "/external/v1/artifacts/art_m8_registered/content").await;
    assert_eq!(content_status, StatusCode::OK);
    assert_eq!(content, b"artifact from adapter");
}

#[tokio::test]
async fn unsupported_capabilities_degrade_independently_without_forged_facts() {
    let _scope = GenericClientTestScope::new().await;
    let state = test_state("m8_degradation").await;
    let session_id = create_session(state.clone()).await;
    let (turn_id, _) = submit_turn(state.clone(), &session_id, "cannot interrupt").await;

    let (started_status, _) = post_json(
        state.clone(),
        "/internal/v1/events",
        None,
        json!({
            "event_id":"evt_m8_started_for_interrupt",
            "session_id":session_id,
            "turn_id":turn_id,
            "source":"agent_adapter",
            "client_type":"generic",
            "type":"turn.started",
            "time":"2026-04-25T12:00:00Z",
            "seq":30,
            "payload":{}
        }),
    )
    .await;
    assert_eq!(started_status, StatusCode::OK);

    let (interrupt_status, interrupt_body) = post_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/interrupt"),
        Some(TOKEN),
        json!({}),
    )
    .await;
    assert_eq!(interrupt_status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(interrupt_body["error"]["code"], "capability_unavailable");

    let (events_status, events_body) = get_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/turns/{turn_id}/events"),
    )
    .await;
    assert_eq!(events_status, StatusCode::OK);
    let event_types: Vec<&str> = events_body["data"]["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|event| event["type"].as_str().unwrap())
        .collect();
    assert!(!event_types.contains(&"turn.interrupt_requested"));

    let (artifacts_status, artifacts_body) = get_json(
        state,
        &format!("/external/v1/sessions/{session_id}/artifacts"),
    )
    .await;
    assert_eq!(artifacts_status, StatusCode::OK);
    assert_eq!(
        artifacts_body["data"]["artifacts"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
}
