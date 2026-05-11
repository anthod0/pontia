#[path = "support/http.rs"]
mod http;
#[path = "support/task_state.rs"]
mod task_state;
#[path = "support/tmux.rs"]
mod tmux;

use axum::http::StatusCode;
use http::{get_json, post_json, post_json_with_idempotency};
use serde_json::{Value, json};
use task_state::test_state;
use tmux::TmuxSessionGuard;

#[tokio::test]
async fn confirm_workspace_dispatches_pending_task() {
    let state = test_state().await;
    let workspace = tempfile::tempdir().expect("workspace");
    let canonical = std::fs::canonicalize(workspace.path()).expect("canonical");

    let (status, body) = post_json(
        state.clone(),
        "/external/v1/tasks",
        json!({"input":"confirm me", "client_type":"generic"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let task_id = body["data"]["task"]["task_id"].as_str().unwrap();
    assert_eq!(body["data"]["task"]["state"], "needs_confirmation");

    let (status, body) = post_json(
        state.clone(),
        &format!("/external/v1/tasks/{task_id}/confirm-workspace"),
        json!({"workspace": workspace.path().display().to_string(), "client_type":"generic"}),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let task = &body["data"]["task"];
    let session_id = task["session_id"].as_str().expect("session id");
    let _runtime_guard = TmuxSessionGuard::for_session(session_id);
    assert_eq!(task["state"], "queued");
    assert_eq!(task["routing_state"], "confirmed");
    assert!(task["workspace_id"].as_str().unwrap().starts_with("wks_"));
    assert!(task["turn_id"].as_str().unwrap().starts_with("turn_"));

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

    let (status, events_body) =
        get_json(state, &format!("/external/v1/tasks/{task_id}/events")).await;
    assert_eq!(status, StatusCode::OK);
    let events = events_body["data"]["events"].as_array().unwrap();
    assert!(
        events
            .iter()
            .any(|event| event["event_type"] == "task.workspace_confirmed")
    );
}

#[tokio::test]
async fn confirm_workspace_rejects_already_dispatched_task() {
    let state = test_state().await;
    let workspace = tempfile::tempdir().expect("workspace");

    let (status, body) = post_json(
        state.clone(),
        "/external/v1/tasks",
        json!({
            "input":"already dispatched",
            "workspace": workspace.path().display().to_string(),
            "client_type":"generic"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let task = &body["data"]["task"];
    let task_id = task["task_id"].as_str().unwrap();
    let session_id = task["session_id"].as_str().unwrap();
    let _runtime_guard = TmuxSessionGuard::for_session(session_id);

    let (status, body) = post_json(
        state,
        &format!("/external/v1/tasks/{task_id}/confirm-workspace"),
        json!({"workspace": workspace.path().display().to_string(), "client_type":"generic"}),
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "state_conflict");
}

#[tokio::test]
async fn cancelling_pending_confirmation_task_marks_it_cancelled() {
    let state = test_state().await;

    let (status, body) = post_json(
        state.clone(),
        "/external/v1/tasks",
        json!({"input":"cancel before routing", "client_type":"generic"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let task_id = body["data"]["task"]["task_id"].as_str().unwrap();

    let (status, body) = post_json_with_idempotency(
        state.clone(),
        &format!("/external/v1/tasks/{task_id}/cancel"),
        json!({}),
        Some("cancel-task-key"),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["task"]["state"], "cancelled");
    assert_eq!(body["data"]["task"]["turn_id"], Value::Null);

    let (status, events_body) =
        get_json(state, &format!("/external/v1/tasks/{task_id}/events")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        events_body["data"]["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|event| event["event_type"] == "task.cancelled")
    );
}
