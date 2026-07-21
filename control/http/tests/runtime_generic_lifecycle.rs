use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use pontia_agent_clients::AgentClientCapabilities;
use pontia_application::{AppState, RuntimeObservationService};
use pontia_http as http;
use serde_json::{Value, json};
use sqlx::Row;
use tower::ServiceExt;

#[path = "support/generic_client.rs"]
mod generic_client;
#[path = "support/test_app.rs"]
mod test_app;

use generic_client::GenericClientTestScope;
use test_app::TestApp;

const TOKEN: &str = "test-token";

async fn test_state(name: &str) -> AppState {
    TestApp::builder()
        .database_name(format!("{name}.db"))
        .external_api_token(Some(TOKEN.to_string()))
        .build_state()
        .await
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

async fn binding_metadata(state: &AppState, session_id: &str) -> Value {
    let row = sqlx::query("SELECT metadata FROM runtime_bindings WHERE session_id = ?")
        .bind(session_id)
        .fetch_one(&state.db())
        .await
        .expect("runtime binding");
    let metadata: String = row.try_get("metadata").expect("metadata");
    serde_json::from_str(&metadata).expect("metadata json")
}

async fn create_session_with_body(state: AppState, body: Value) -> String {
    let (status, body) = request(state, "POST", "/external/v1/sessions", Some(body)).await;
    assert_eq!(status, StatusCode::CREATED, "{body:?}");
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
        Some(json!({"input":"work through generic"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body:?}");
    body["data"]["inbox_message"]["turn_id"]
        .as_str()
        .expect("turn id")
        .to_string()
}

#[tokio::test]
async fn generic_runtime_handle_includes_handle_role_and_short_session_id() {
    let scope = GenericClientTestScope::new().await;
    let state = test_state("generic_named_runtime").await;
    let workspace = tempfile::tempdir().expect("workspace");

    let session_id = create_session_with_body(
        state.clone(),
        json!({
            "client_type": "generic",
            "workspace": workspace.path().display().to_string(),
            "handle": "@reviewer",
            "role": "execution reviewer"
        }),
    )
    .await;
    let metadata = binding_metadata(&state, &session_id).await;
    let runtime_handle = scope.runtime_handle(&state, &session_id).await;
    let id_body = session_id.rsplit('_').next().unwrap_or(&session_id);
    let short_id = id_body[id_body.len() - 8..].to_string();

    assert_eq!(metadata["backend"], "in_process");
    assert_eq!(metadata["handle"], "@reviewer");
    assert_eq!(metadata["role"], "execution reviewer");
    assert_eq!(
        runtime_handle,
        format!("generic:reviewer:execution_reviewer:{short_id}")
    );
    assert!(scope.is_runtime_alive(&runtime_handle));
}

#[tokio::test]
async fn generic_terminate_and_restart_update_runtime_lifecycle() {
    let scope = GenericClientTestScope::new().await;
    let state = test_state("generic_restart").await;
    let session_id =
        create_session_with_body(state.clone(), json!({"client_type":"generic"})).await;
    let first = binding_metadata(&state, &session_id).await;
    let runtime_handle = scope.runtime_handle(&state, &session_id).await;
    assert!(scope.is_runtime_alive(&runtime_handle));

    let (status, body) = request(
        state.clone(),
        "POST",
        &format!("/external/v1/sessions/{session_id}/restart"),
        None,
    )
    .await;
    let second = binding_metadata(&state, &session_id).await;

    assert_eq!(status, StatusCode::OK, "{body:?}");
    assert_eq!(body["data"]["session"]["state"], "idle");
    assert_eq!(
        scope.runtime_handle(&state, &session_id).await,
        runtime_handle
    );
    assert_eq!(second["restart_count"], 1);
    assert_ne!(first["runtime_instance_id"], second["runtime_instance_id"]);
    assert_ne!(first["started_at"], second["started_at"]);
    assert!(scope.is_runtime_alive(&runtime_handle));

    let (status, body) = request(
        state.clone(),
        "DELETE",
        &format!("/external/v1/sessions/{session_id}"),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body:?}");
    assert_eq!(body["data"]["session"]["state"], "exited");
    assert!(!scope.is_runtime_alive(&runtime_handle));
}

#[tokio::test]
async fn observe_missing_generic_runtime_projects_session_error() {
    let scope = GenericClientTestScope::new().await;
    let state = test_state("generic_observe_session_error").await;
    let session_id =
        create_session_with_body(state.clone(), json!({"client_type":"generic"})).await;
    scope.reset_runtime_registry();

    RuntimeObservationService::new(state.db())
        .observe_session(&session_id)
        .await
        .expect("observe runtime");

    let (status, body) = request(
        state,
        "GET",
        &format!("/external/v1/sessions/{session_id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");
    assert_eq!(body["data"]["session"]["state"], "error");
}

#[tokio::test]
async fn observe_missing_generic_runtime_does_not_fail_a_terminal_branch_leaf() {
    let scope = GenericClientTestScope::new().await;
    let state = test_state("generic_observe_terminal_leaf").await;
    let session_id =
        create_session_with_body(state.clone(), json!({"client_type":"generic"})).await;
    sqlx::query(
        r#"INSERT INTO turns (turn_id, session_id, state, metadata)
           VALUES ('turn_terminal_leaf', ?, 'completed', '{}')"#,
    )
    .bind(&session_id)
    .execute(&state.db())
    .await
    .expect("insert terminal branch leaf");
    sqlx::query("UPDATE sessions SET current_turn_id = 'turn_terminal_leaf' WHERE session_id = ?")
        .bind(&session_id)
        .execute(&state.db())
        .await
        .expect("set current branch leaf");
    scope.reset_runtime_registry();

    RuntimeObservationService::new(state.db())
        .observe_session(&session_id)
        .await
        .expect("observe runtime");

    let (status, body) = request(
        state,
        "GET",
        &format!("/external/v1/sessions/{session_id}/events"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");
    assert!(
        body["data"]["events"]
            .as_array()
            .unwrap()
            .iter()
            .all(|event| event["type"] != "turn.failed")
    );
}

#[tokio::test]
async fn observe_missing_generic_runtime_fails_active_turn() {
    let scope = GenericClientTestScope::new()
        .await
        .with_capabilities(AgentClientCapabilities::pi_m0_default())
        .auto_start_turn();
    let state = test_state("generic_observe_turn_failed").await;
    let session_id =
        create_session_with_body(state.clone(), json!({"client_type":"generic"})).await;
    let turn_id = submit_turn(state.clone(), &session_id).await;
    assert_eq!(scope.recorded_inputs().len(), 1);
    scope.reset_runtime_registry();

    RuntimeObservationService::new(state.db())
        .observe_session(&session_id)
        .await
        .expect("observe runtime");

    let (status, body) = request(
        state,
        "GET",
        &format!("/external/v1/sessions/{session_id}/turns/{turn_id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");
    assert_eq!(body["data"]["turn"]["state"], "failed");
}
