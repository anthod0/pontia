#[path = "support/generic_client.rs"]
mod generic_client;
#[path = "support/http.rs"]
mod http;
#[path = "support/task_state.rs"]
mod task_state;

use axum::http::StatusCode;
use generic_client::GenericClientTestScope;
use http::{get_json, post_json, post_json_with_idempotency};
use serde_json::{Value, json};
use task_state::test_state;

#[tokio::test]
async fn create_task_without_workspace_persists_global_task_for_confirmation() {
    let _scope = GenericClientTestScope::new().await;
    let state = test_state().await;

    let (status, body) = post_json(
        state.clone(),
        "/external/v1/tasks",
        json!({"input":"find the right workspace", "client_type":"generic"}),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    let task = &body["data"]["task"];
    let task_id = task["task_id"].as_str().expect("task id");
    assert!(task_id.starts_with("task_"));
    assert_eq!(task["state"], "needs_confirmation");
    assert_eq!(task["routing_state"], "ambiguous");
    assert_eq!(task["workspace_id"], Value::Null);
    assert_eq!(task["session_id"], Value::Null);
    assert_eq!(task["turn_id"], Value::Null);

    let (status, body) = get_json(state, "/external/v1/tasks").await;
    assert_eq!(status, StatusCode::OK);
    let tasks = body["data"]["tasks"].as_array().unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0]["task_id"], task_id);
}

#[tokio::test]
async fn task_creation_idempotency_returns_same_task_for_replayed_key() {
    let _scope = GenericClientTestScope::new().await;
    let state = test_state().await;
    let request = json!({"input":"retry-safe task", "client_type":"generic"});

    let (status, first) = post_json_with_idempotency(
        state.clone(),
        "/external/v1/tasks",
        request.clone(),
        Some("task-retry-key"),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, second) = post_json_with_idempotency(
        state.clone(),
        "/external/v1/tasks",
        request,
        Some("task-retry-key"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        first["data"]["task"]["task_id"],
        second["data"]["task"]["task_id"]
    );

    let (status, body) = get_json(state, "/external/v1/tasks").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["tasks"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn invalid_task_client_type_returns_error_without_creating_task() {
    let _scope = GenericClientTestScope::new().await;
    let state = test_state().await;

    let (status, body) = post_json(
        state.clone(),
        "/external/v1/tasks",
        json!({"input":"bad client", "client_type":"unsupported"}),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "invalid_request");

    let (status, body) = get_json(state, "/external/v1/tasks").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["tasks"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn create_task_with_workspace_routes_to_session_and_links_created_turn() {
    let _scope = GenericClientTestScope::new().await;
    let state = test_state().await;
    let workspace = tempfile::tempdir().expect("workspace");
    let canonical = std::fs::canonicalize(workspace.path()).expect("canonical");

    let (status, body) = post_json(
        state.clone(),
        "/external/v1/tasks",
        json!({
            "input":"run this globally",
            "workspace": workspace.path().display().to_string(),
            "client_type":"generic",
            "metadata":{"source":"test"}
        }),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    let task = &body["data"]["task"];
    let session_id = task["session_id"].as_str().expect("session id");
    let turn_id = task["turn_id"].as_str().expect("turn id");
    assert_eq!(task["state"], "queued");
    assert_eq!(task["routing_state"], "matched");
    assert_eq!(task["input"], "run this globally");
    assert!(task["workspace_id"].as_str().unwrap().starts_with("wks_"));

    let (status, session_body) = get_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        session_body["data"]["session"]["workspace"],
        canonical.display().to_string()
    );

    let (status, turn_body) = get_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/turns/{turn_id}"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        turn_body["data"]["turn"]["input"]["summary"],
        "run this globally"
    );

    let (status, task_body) = get_json(
        state,
        &format!("/external/v1/tasks/{}", task["task_id"].as_str().unwrap()),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(task_body["data"]["task"]["turn_id"], turn_id);
}
