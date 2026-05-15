#[path = "support/events.rs"]
mod events;
#[path = "support/generic_client.rs"]
mod generic_client;
#[path = "support/http.rs"]
mod http;
#[path = "support/task_state.rs"]
mod task_state;

use axum::http::StatusCode;
use events::{event_body, post_internal_event};
use generic_client::GenericClientTestScope;
use http::{get_json, post_json, post_json_with_idempotency};
use serde_json::{Value, json};
use task_state::test_state;

#[tokio::test]
async fn task_events_endpoint_returns_task_lifecycle_history() {
    let _scope = GenericClientTestScope::new().await;
    let state = test_state().await;

    let (status, body) = post_json(
        state.clone(),
        "/external/v1/tasks",
        json!({"input":"show my task events", "client_type":"generic"}),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    let task_id = body["data"]["task"]["task_id"].as_str().expect("task id");

    let (status, body) = get_json(state, &format!("/external/v1/tasks/{task_id}/events")).await;

    assert_eq!(status, StatusCode::OK);
    let events = body["data"]["events"].as_array().expect("events");
    assert!(
        events
            .iter()
            .any(|event| event["event_type"] == "task.created")
    );
    assert!(
        events
            .iter()
            .any(|event| event["event_type"] == "task.routing_ambiguous")
    );
    assert_eq!(events[0]["task_id"], task_id);
    assert!(events[0]["event_id"].as_str().unwrap().starts_with("evt_"));
    assert!(events[0]["payload"].is_object());
    assert!(events[0]["created_at"].as_str().is_some());
}

#[tokio::test]
async fn task_events_endpoint_returns_not_found_for_missing_task() {
    let _scope = GenericClientTestScope::new().await;
    let state = test_state().await;

    let (status, body) = get_json(state, "/external/v1/tasks/task_missing/events").await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "not_found");
}

#[tokio::test]
async fn task_state_follows_turn_lifecycle() {
    let _scope = GenericClientTestScope::new().await;
    let state = test_state().await;
    let workspace = tempfile::tempdir().expect("workspace");

    let (status, body) = post_json(
        state.clone(),
        "/external/v1/tasks",
        json!({
            "input":"track lifecycle",
            "workspace": workspace.path().display().to_string(),
            "client_type":"generic"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let task_id = body["data"]["task"]["task_id"].as_str().unwrap();
    let session_id = body["data"]["task"]["session_id"].as_str().unwrap();
    let turn_id = body["data"]["task"]["turn_id"].as_str().unwrap();

    let (status, _) = post_internal_event(
        state.clone(),
        event_body("evt_task_started", "turn.started", session_id, turn_id, 3),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, body) = get_json(state.clone(), &format!("/external/v1/tasks/{task_id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["task"]["state"], "running");

    let (status, _) = post_internal_event(
        state.clone(),
        event_body(
            "evt_task_completed",
            "turn.completed",
            session_id,
            turn_id,
            4,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, body) = get_json(state.clone(), &format!("/external/v1/tasks/{task_id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["task"]["state"], "completed");

    let (status, body) = post_internal_event(
        state.clone(),
        event_body(
            "evt_task_completed",
            "turn.completed",
            session_id,
            turn_id,
            4,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["duplicate"], true);

    let (status, body) = get_json(state, &format!("/external/v1/tasks/{task_id}/events")).await;
    assert_eq!(status, StatusCode::OK);
    let events = body["data"]["events"].as_array().unwrap();
    assert_eq!(
        events
            .iter()
            .filter(|event| event["event_type"] == "task.completed")
            .count(),
        1
    );
    assert!(
        events
            .iter()
            .any(|event| event["event_type"] == "task.running")
    );
}

#[tokio::test]
async fn task_interrupt_delegates_to_active_turn_and_updates_task_state() {
    let _scope = GenericClientTestScope::new().await;
    let state = test_state().await;
    let workspace = tempfile::tempdir().expect("workspace");

    let (status, body) = post_json(
        state.clone(),
        "/external/v1/tasks",
        json!({
            "input":"interrupt via task",
            "workspace": workspace.path().display().to_string(),
            "client_type":"generic"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let task = &body["data"]["task"];
    let task_id = task["task_id"].as_str().unwrap();
    let session_id = task["session_id"].as_str().unwrap();
    let turn_id = task["turn_id"].as_str().unwrap();
    let metadata: String =
        sqlx::query_scalar("SELECT metadata FROM runtime_bindings WHERE session_id = ?")
            .bind(session_id)
            .fetch_one(&state.db)
            .await
            .expect("runtime binding metadata");
    let mut metadata: Value = serde_json::from_str(&metadata).expect("metadata json");
    metadata["capabilities"]["interrupt"] = Value::Bool(true);
    sqlx::query("UPDATE runtime_bindings SET metadata = ? WHERE session_id = ?")
        .bind(metadata.to_string())
        .bind(session_id)
        .execute(&state.db)
        .await
        .expect("enable interrupt capability");

    let (status, _) = post_internal_event(
        state.clone(),
        event_body(
            "evt_task_interrupt_started",
            "turn.started",
            session_id,
            turn_id,
            3,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, body) = post_json_with_idempotency(
        state,
        &format!("/external/v1/tasks/{task_id}/interrupt"),
        json!({}),
        Some("interrupt-task-key"),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["task"]["state"], "cancelled");
}
