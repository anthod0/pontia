#[path = "support/http.rs"]
mod http;
#[path = "support/planner_state.rs"]
mod planner_state;

use axum::http::StatusCode;
use http::{get_json, post_json, post_json_with_idempotency};
use planner_state::planner_test_state;
use serde_json::{Value, json};

#[tokio::test]
async fn planner_resolved_task_creates_dispatch_handoff_without_direct_dispatch() {
    let state = planner_test_state().await;
    let workspace = tempfile::tempdir().expect("workspace");
    let canonical = std::fs::canonicalize(workspace.path()).expect("canonical");

    let (status, body) = post_json(
        state.clone(),
        "/external/v1/tasks",
        json!({
            "input":"resolve this workspace",
            "client_type":"generic",
            "metadata": {
                "planner_decision": {
                    "status":"resolved",
                    "workspace": {
                        "canonical_path": canonical.display().to_string(),
                        "confidence": 0.84,
                        "reason": "fake planner matched requested workspace"
                    },
                    "reason":"fake planner resolved"
                }
            }
        }),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    let task = &body["data"]["task"];
    let task_id = task["task_id"].as_str().expect("task id");
    assert_eq!(task["state"], "routing");
    assert_eq!(task["routing_state"], "matched");
    assert_eq!(task["routing_confidence"], 0.84);
    assert!(task["workspace_id"].as_str().unwrap().starts_with("wks_"));
    assert_eq!(task["session_id"], Value::Null);
    assert_eq!(task["turn_id"], Value::Null);

    let (status, events_body) =
        get_json(state, &format!("/external/v1/tasks/{task_id}/events")).await;
    assert_eq!(status, StatusCode::OK);
    let events = events_body["data"]["events"].as_array().unwrap();
    assert!(
        events
            .iter()
            .any(|event| event["event_type"] == "task.planning_started")
    );
    assert!(
        events
            .iter()
            .any(|event| event["event_type"] == "task.planning_resolved")
    );
    let handoff = events
        .iter()
        .find(|event| event["event_type"] == "task.dispatch_handoff_created")
        .expect("handoff event");
    assert_eq!(handoff["payload"]["task_id"], task_id);
    assert_eq!(
        handoff["payload"]["canonical_path"],
        canonical.display().to_string()
    );
}

#[tokio::test]
async fn planner_needs_input_keeps_task_waiting_for_user_information() {
    let state = planner_test_state().await;

    let (status, body) = post_json(
        state.clone(),
        "/external/v1/tasks",
        json!({"input":"which project?", "client_type":"generic"}),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    let task = &body["data"]["task"];
    let task_id = task["task_id"].as_str().expect("task id");
    assert_eq!(task["state"], "needs_confirmation");
    assert_eq!(task["routing_state"], "ambiguous");
    assert!(
        task["routing_reason"]
            .as_str()
            .unwrap()
            .contains("Which workspace")
    );

    let (status, events_body) =
        get_json(state, &format!("/external/v1/tasks/{task_id}/events")).await;
    assert_eq!(status, StatusCode::OK);
    let events = events_body["data"]["events"].as_array().unwrap();
    assert!(
        events
            .iter()
            .any(|event| event["event_type"] == "task.planning_needs_input")
    );
}

#[tokio::test]
async fn planner_input_resumes_planning_and_creates_dispatch_handoff() {
    let state = planner_test_state().await;
    let workspace = tempfile::tempdir().expect("workspace");
    let canonical = std::fs::canonicalize(workspace.path()).expect("canonical");

    let (status, body) = post_json(
        state.clone(),
        "/external/v1/tasks",
        json!({"input":"needs a workspace hint", "client_type":"generic"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let task_id = body["data"]["task"]["task_id"].as_str().unwrap();

    let (status, body) = post_json_with_idempotency(
        state.clone(),
        &format!("/external/v1/tasks/{task_id}/planner-input"),
        json!({"message": canonical.display().to_string(), "client_type":"generic"}),
        Some("planner-input-key"),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let task = &body["data"]["task"];
    assert_eq!(task["state"], "routing");
    assert_eq!(task["routing_state"], "matched");
    assert_eq!(task["turn_id"], Value::Null);

    let (status, replay) = post_json_with_idempotency(
        state.clone(),
        &format!("/external/v1/tasks/{task_id}/planner-input"),
        json!({"message": canonical.display().to_string(), "client_type":"generic"}),
        Some("planner-input-key"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replay["data"]["task"]["task_id"], task_id);

    let (status, events_body) =
        get_json(state, &format!("/external/v1/tasks/{task_id}/events")).await;
    assert_eq!(status, StatusCode::OK);
    let events = events_body["data"]["events"].as_array().unwrap();
    assert!(
        events
            .iter()
            .any(|event| event["event_type"] == "task.planning_input_received")
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| event["event_type"] == "task.dispatch_handoff_created")
            .count(),
        1
    );
}

#[tokio::test]
async fn planner_failure_falls_back_to_manual_confirmation() {
    let state = planner_test_state().await;

    let (status, body) = post_json(
        state.clone(),
        "/external/v1/tasks",
        json!({
            "input":"planner cannot decide",
            "client_type":"generic",
            "metadata": {"planner_decision": {"status":"failed", "reason":"no useful evidence"}}
        }),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    let task = &body["data"]["task"];
    let task_id = task["task_id"].as_str().expect("task id");
    assert_eq!(task["state"], "needs_confirmation");
    assert_eq!(task["routing_state"], "failed");
    assert_eq!(task["routing_reason"], "no useful evidence");

    let (status, events_body) =
        get_json(state, &format!("/external/v1/tasks/{task_id}/events")).await;
    assert_eq!(status, StatusCode::OK);
    let events = events_body["data"]["events"].as_array().unwrap();
    assert!(
        events
            .iter()
            .any(|event| event["event_type"] == "task.planning_failed")
    );
}
