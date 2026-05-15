#[path = "support/generic_client.rs"]
mod generic_client;
#[path = "support/http.rs"]
mod http;
#[path = "support/task_state.rs"]
mod task_state;

use axum::http::StatusCode;
use generic_client::GenericClientTestScope;
use http::{get_json, post_json, post_json_with_idempotency};
use serde_json::json;
use task_state::test_state;

async fn enable_generic_planner_profile(state: &llmparty::application::AppState) {
    sqlx::query(
        "UPDATE execution_profiles SET supported_client_types = ? WHERE profile_id = 'planner'",
    )
    .bind(json!(["pi", "claude_code", "generic"]).to_string())
    .execute(&state.db)
    .await
    .expect("enable generic planner profile");
}

#[tokio::test]
async fn dag_task_api_creates_task_links_workspace_and_starts_generic_planner_turn() {
    let _scope = GenericClientTestScope::new().await;
    let state = test_state().await;
    enable_generic_planner_profile(&state).await;
    let workspace = tempfile::tempdir().expect("workspace");
    let workspace_path = workspace.path().display().to_string();

    let (status, body) = post_json(
        state.clone(),
        "/external/v1/dag-tasks",
        json!({
            "input": "Create demo file",
            "workspace": workspace_path,
            "client_type": "generic",
            "metadata": {"source": "test"}
        }),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED, "{body:#}");
    let task = &body["data"]["task"];
    assert_eq!(task["state"], "planning");
    assert_eq!(task["routing_state"], "matched");
    assert_eq!(task["metadata"]["dag_managed"], true);
    assert_eq!(task["metadata"]["mode"], "dag");
    assert!(task["workspace_id"].as_str().is_some());
    assert!(task["session_id"].as_str().is_none());
    assert!(task["turn_id"].as_str().is_none());

    let planning = &body["data"]["planning_turn"];
    assert_eq!(planning["task_id"], task["task_id"]);
    assert_eq!(planning["profile_id"], "planner");
    let session_id = planning["session_id"].as_str().expect("session id");
    let turn_id = planning["turn_id"].as_str().expect("turn id");

    let (_session_status, session_body) = get_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}"),
    )
    .await;
    let session = &session_body["data"]["session"];
    assert_eq!(session["client_type"], "generic");
    assert_eq!(session["workspace_id"], task["workspace_id"]);
    assert_eq!(session["metadata"]["dag_managed"], true);
    assert_eq!(session["metadata"]["dag_planning_role"], "planner");

    let (_turn_status, turn_body) = get_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/turns/{turn_id}"),
    )
    .await;
    let turn = &turn_body["data"]["turn"];
    assert_eq!(turn["metadata"]["dag_managed"], true);
    assert_eq!(turn["metadata"]["dag_planning_role"], "planner");
    assert!(
        turn["input"]["summary"]
            .as_str()
            .expect("prompt")
            .contains("Plan task")
    );

    let (_events_status, events_body) = get_json(
        state,
        &format!(
            "/external/v1/tasks/{}/events",
            task["task_id"].as_str().unwrap()
        ),
    )
    .await;
    let event_types: Vec<&str> = events_body["data"]["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|event| event["event_type"].as_str().unwrap())
        .collect();
    assert!(event_types.contains(&"task.created"));
    assert!(event_types.contains(&"task.workspace_matched"));
    assert!(event_types.contains(&"task.planning_started"));
}

#[tokio::test]
async fn dag_task_api_requires_workspace_and_is_idempotent() {
    let _scope = GenericClientTestScope::new().await;
    let state = test_state().await;
    enable_generic_planner_profile(&state).await;

    let (missing_status, missing_body) = post_json(
        state.clone(),
        "/external/v1/dag-tasks",
        json!({"input":"Plan only", "client_type":"generic"}),
    )
    .await;
    assert_eq!(missing_status, StatusCode::BAD_REQUEST);
    assert_eq!(missing_body["error"]["code"], "invalid_request");

    let workspace = tempfile::tempdir().expect("workspace");
    let request = json!({
        "input": "Create demo file",
        "workspace": workspace.path().display().to_string(),
        "client_type": "generic",
        "metadata": {}
    });

    let (first_status, first_body) = post_json_with_idempotency(
        state.clone(),
        "/external/v1/dag-tasks",
        request.clone(),
        Some("dag-task-key"),
    )
    .await;
    assert_eq!(first_status, StatusCode::CREATED, "{first_body:#}");
    let (second_status, second_body) = post_json_with_idempotency(
        state,
        "/external/v1/dag-tasks",
        request,
        Some("dag-task-key"),
    )
    .await;
    assert_eq!(second_status, StatusCode::OK, "{second_body:#}");
    assert_eq!(
        second_body["data"]["task"]["task_id"],
        first_body["data"]["task"]["task_id"]
    );
    assert_eq!(
        second_body["data"]["planning_turn"]["turn_id"],
        first_body["data"]["planning_turn"]["turn_id"]
    );
}
