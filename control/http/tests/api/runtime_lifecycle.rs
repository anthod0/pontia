use crate::test_app::TestApp;
use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use pontia_application::AppState;
use pontia_http as http;
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::generic_client::GenericClientTestScope;

const TOKEN: &str = "test-token";

async fn test_state() -> AppState {
    TestApp::builder()
        .database_name("runtime_lifecycle.db")
        .external_api_token(Some(TOKEN.to_string()))
        .build_state()
        .await
}

async fn request(
    state: AppState,
    method: &str,
    uri: &str,
    token: Option<&str>,
    idempotency_key: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(token) = token {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
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
    let (status, body) = request(
        state,
        "POST",
        "/external/v1/sessions",
        Some(TOKEN),
        None,
        Some(json!({"client_type":"generic"})),
    )
    .await;
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
        Some(TOKEN),
        None,
        Some(json!({"input":"work"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    body["data"]["inbox_message"]["turn_id"]
        .as_str()
        .expect("turn id")
        .to_string()
}

#[tokio::test]
async fn interrupt_current_turn_returns_capability_unavailable_for_generic_runtime() {
    let _scope = GenericClientTestScope::new().await;
    let state = test_state().await;
    let session_id = create_session(state.clone()).await;
    let turn_id = submit_turn(state.clone(), &session_id).await;

    let (status, body) = request(
        state.clone(),
        "POST",
        &format!("/external/v1/sessions/{session_id}/interrupt"),
        Some(TOKEN),
        None,
        None,
    )
    .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"]["code"], "capability_unavailable");

    let (events_status, events_body) = request(
        state,
        "GET",
        &format!("/external/v1/sessions/{session_id}/turns/{turn_id}/events"),
        Some(TOKEN),
        None,
        None,
    )
    .await;
    assert_eq!(events_status, StatusCode::OK);
    let types: Vec<&str> = events_body["data"]["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|event| event["type"].as_str().unwrap())
        .collect();
    assert_eq!(types, vec!["turn.created", "turn.queued"]);
}

#[tokio::test]
async fn interrupt_specified_turn_returns_capability_unavailable_for_generic_runtime() {
    let _scope = GenericClientTestScope::new().await;
    let state = test_state().await;
    let session_id = create_session(state.clone()).await;
    let turn_id = submit_turn(state.clone(), &session_id).await;

    let (status, body) = request(
        state,
        "POST",
        &format!("/external/v1/sessions/{session_id}/turns/{turn_id}/interrupt"),
        Some(TOKEN),
        Some("interrupt-once"),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"]["code"], "capability_unavailable");
}

#[tokio::test]
async fn terminate_session_requests_runtime_shutdown_emits_one_exit_event_and_is_idempotent() {
    let _scope = GenericClientTestScope::new().await;
    let state = test_state().await;
    let session_id = create_session(state.clone()).await;

    let first = request(
        state.clone(),
        "DELETE",
        &format!("/external/v1/sessions/{session_id}"),
        Some(TOKEN),
        Some("terminate-once"),
        None,
    )
    .await;
    let second = request(
        state.clone(),
        "DELETE",
        &format!("/external/v1/sessions/{session_id}"),
        Some(TOKEN),
        Some("terminate-once"),
        None,
    )
    .await;
    let third = request(
        state.clone(),
        "DELETE",
        &format!("/external/v1/sessions/{session_id}"),
        Some(TOKEN),
        None,
        None,
    )
    .await;

    assert_eq!(first.0, StatusCode::OK);
    assert_eq!(second.0, StatusCode::OK);
    assert_eq!(third.0, StatusCode::OK);
    assert_eq!(second.1["data"], first.1["data"]);
    assert_eq!(first.1["data"]["session"]["state"], "exited");
    assert_eq!(third.1["data"]["session"]["state"], "exited");
    assert_eq!(first.1["data"]["session"]["current_turn_id"], Value::Null);

    let (events_status, events_body) = request(
        state,
        "GET",
        &format!("/external/v1/sessions/{session_id}/events"),
        Some(TOKEN),
        None,
        None,
    )
    .await;
    assert_eq!(events_status, StatusCode::OK);
    let exited_count = events_body["data"]["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|event| event["type"] == "session.exited")
        .count();
    assert_eq!(exited_count, 1);
}

#[tokio::test]
async fn restart_idle_session_with_a_sticky_branch_leaf_runs_a_new_start_cycle() {
    let _scope = GenericClientTestScope::new().await;
    let state = test_state().await;
    let session_id = create_session(state.clone()).await;
    sqlx::query(
        r#"INSERT INTO turns (turn_id, session_id, state, metadata)
           VALUES ('turn_completed_leaf', ?, 'completed', '{}')"#,
    )
    .bind(&session_id)
    .execute(&state.db())
    .await
    .expect("insert completed branch leaf");
    sqlx::query("UPDATE sessions SET current_turn_id = 'turn_completed_leaf' WHERE session_id = ?")
        .bind(&session_id)
        .execute(&state.db())
        .await
        .expect("make completed Turn the current branch leaf");

    let first = request(
        state.clone(),
        "POST",
        &format!("/external/v1/sessions/{session_id}/restart"),
        Some(TOKEN),
        Some("restart-once"),
        None,
    )
    .await;
    let second = request(
        state.clone(),
        "POST",
        &format!("/external/v1/sessions/{session_id}/restart"),
        Some(TOKEN),
        Some("restart-once"),
        None,
    )
    .await;

    assert_eq!(first.0, StatusCode::OK);
    assert_eq!(second.0, StatusCode::OK);
    assert_eq!(second.1["data"], first.1["data"]);
    assert_eq!(first.1["data"]["session"]["state"], "idle");
    assert_eq!(
        first.1["data"]["session"]["current_turn_id"],
        "turn_completed_leaf"
    );
    assert_eq!(
        first.1["data"]["session"]["capabilities"]["interrupt"],
        false
    );

    let (events_status, events_body) = request(
        state,
        "GET",
        &format!("/external/v1/sessions/{session_id}/events"),
        Some(TOKEN),
        None,
        None,
    )
    .await;
    assert_eq!(events_status, StatusCode::OK);
    let types: Vec<&str> = events_body["data"]["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|event| event["type"].as_str().unwrap())
        .collect();
    assert_eq!(
        types,
        vec![
            "session.created",
            "session.starting",
            "session.started",
            "session.ready",
            "session.starting",
            "session.started",
            "session.ready",
        ]
    );
}

#[tokio::test]
async fn restart_rejects_replacing_the_runtime_while_a_turn_is_active() {
    let _scope = GenericClientTestScope::new().await;
    let state = test_state().await;
    let session_id = create_session(state.clone()).await;
    let turn_id = submit_turn(state.clone(), &session_id).await;
    let original_runtime_instance_id: String =
        sqlx::query_scalar("SELECT runtime_instance_id FROM runtime_bindings WHERE session_id = ?")
            .bind(&session_id)
            .fetch_one(&state.db())
            .await
            .expect("runtime binding");

    let (status, body) = request(
        state.clone(),
        "POST",
        &format!("/external/v1/sessions/{session_id}/restart"),
        Some(TOKEN),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body:?}");
    assert_eq!(body["error"]["code"], "state_conflict");

    let persisted_runtime_instance_id: String =
        sqlx::query_scalar("SELECT runtime_instance_id FROM runtime_bindings WHERE session_id = ?")
            .bind(&session_id)
            .fetch_one(&state.db())
            .await
            .expect("runtime binding");
    assert_eq!(persisted_runtime_instance_id, original_runtime_instance_id);
    let (turn_status, turn_body) = request(
        state,
        "GET",
        &format!("/external/v1/sessions/{session_id}/turns/{turn_id}"),
        Some(TOKEN),
        None,
        None,
    )
    .await;
    assert_eq!(turn_status, StatusCode::OK, "{turn_body:?}");
    assert_eq!(turn_body["data"]["turn"]["state"], "queued");
}

#[tokio::test]
async fn resume_exited_session_runs_resume_cycle_and_is_idempotent() {
    let _scope = GenericClientTestScope::new().await;
    let state = test_state().await;
    let session_id = create_session(state.clone()).await;
    let terminate = request(
        state.clone(),
        "DELETE",
        &format!("/external/v1/sessions/{session_id}"),
        Some(TOKEN),
        None,
        None,
    )
    .await;
    assert_eq!(terminate.0, StatusCode::OK);

    let first = request(
        state.clone(),
        "POST",
        &format!("/external/v1/sessions/{session_id}/resume"),
        Some(TOKEN),
        Some("resume-once"),
        None,
    )
    .await;
    let second = request(
        state.clone(),
        "POST",
        &format!("/external/v1/sessions/{session_id}/resume"),
        Some(TOKEN),
        Some("resume-once"),
        None,
    )
    .await;

    assert_eq!(first.0, StatusCode::OK);
    assert_eq!(second.0, StatusCode::OK);
    assert_eq!(second.1["data"], first.1["data"]);
    assert_eq!(first.1["data"]["session"]["state"], "idle");

    let (events_status, events_body) = request(
        state,
        "GET",
        &format!("/external/v1/sessions/{session_id}/events"),
        Some(TOKEN),
        None,
        None,
    )
    .await;
    assert_eq!(events_status, StatusCode::OK);
    let types: Vec<&str> = events_body["data"]["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|event| event["type"].as_str().unwrap())
        .collect();
    assert_eq!(
        types,
        vec![
            "session.created",
            "session.starting",
            "session.started",
            "session.ready",
            "session.exited",
            "session.resuming",
            "session.started",
            "session.ready",
        ]
    );
}

#[tokio::test]
async fn resume_rejects_non_exited_session() {
    let _scope = GenericClientTestScope::new().await;
    let state = test_state().await;
    let session_id = create_session(state.clone()).await;

    let (status, body) = request(
        state,
        "POST",
        &format!("/external/v1/sessions/{session_id}/resume"),
        Some(TOKEN),
        None,
        None,
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "state_conflict");
}

#[tokio::test]
async fn resume_rejects_error_session() {
    let _scope = GenericClientTestScope::new().await;
    let state = test_state().await;
    let session_id = create_session(state.clone()).await;
    pontia_application::EventIngestService::new(state.db())
        .ingest_reported_event(pontia_core::domain::ReportedEvent::new(
            pontia_core::ids::new_event_id().to_string(),
            session_id.clone(),
            None,
            pontia_core::domain::EventSource::RuntimeManager,
            "generic".to_string(),
            pontia_core::domain::EventType::SessionError,
            json!({ "error": { "message": "boom" } }),
        ))
        .await
        .expect("ingest session error");

    let (status, body) = request(
        state,
        "POST",
        &format!("/external/v1/sessions/{session_id}/resume"),
        Some(TOKEN),
        None,
        None,
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "state_conflict");
}

#[tokio::test]
async fn restart_rejects_terminal_session() {
    let _scope = GenericClientTestScope::new().await;
    let state = test_state().await;
    let session_id = create_session(state.clone()).await;
    let terminate = request(
        state.clone(),
        "DELETE",
        &format!("/external/v1/sessions/{session_id}"),
        Some(TOKEN),
        None,
        None,
    )
    .await;
    assert_eq!(terminate.0, StatusCode::OK);

    let (status, body) = request(
        state,
        "POST",
        &format!("/external/v1/sessions/{session_id}/restart"),
        Some(TOKEN),
        None,
        None,
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "state_conflict");
}
