use crate::{http::get_json, task_state::test_state};
use axum::http::StatusCode;
use pontia_core::ids::new_task_id;
use serde_json::json;

#[tokio::test]
async fn task_events_endpoint_returns_task_lifecycle_history() {
    let state = test_state().await;
    let task_id = new_task_id().to_string();
    sqlx::query("INSERT INTO tasks (task_id, state, input) VALUES (?, 'created', 'show events')")
        .bind(&task_id)
        .execute(&state.db())
        .await
        .expect("insert task");
    sqlx::query(
        "INSERT INTO task_events (event_id, task_id, event_type, payload) VALUES ('evt_test_task_history', ?, 'task.created', ?)",
    )
    .bind(&task_id)
    .bind(json!({"source":"test"}).to_string())
    .execute(&state.db())
    .await
    .expect("insert task event");

    let (status, body) = get_json(state, &format!("/external/v1/tasks/{task_id}/events")).await;

    assert_eq!(status, StatusCode::OK);
    let events = body["data"]["events"].as_array().expect("events");
    assert_eq!(events[0]["task_id"], task_id);
    assert_eq!(events[0]["event_type"], "task.created");
    assert!(events[0]["payload"].is_object());
}

#[tokio::test]
async fn task_events_endpoint_returns_not_found_for_missing_task() {
    let state = test_state().await;

    let (status, body) = get_json(state, "/external/v1/tasks/task_missing/events").await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "not_found");
}
