#![cfg(any())]

use crate::agent_tools_support::*;
use axum::http::StatusCode;
use serde_json::json;

#[tokio::test]
async fn rejects_unknown_tool_and_invalid_requests() {
    let state = test_state().await;

    let (status, body) = post_tool(
        state.clone(),
        "noSuchTool",
        json!({
            "session_id": "sess_missing",
            "turn_id": "turn_missing",
            "runtime_instance_id": "rt_missing",
            "input": {}
        }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "not_found");

    let (status, body) = post_tool(state, "getContext", json!({"input": {}})).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "invalid_request");
}

#[tokio::test]
async fn authorizes_planning_context_from_session_turn_and_runtime_binding() {
    let state = test_state().await;
    insert_task(&state.db(), "task_plan").await;
    insert_dag_session(
        &state.db(),
        "sess_plan",
        "turn_plan",
        "rt_plan",
        json!({
            "dag_managed": true,
            "dag_planning_role": "planner",
            "task_id": "task_plan"
        }),
    )
    .await;

    let (status, body) = post_tool(
        state,
        "getContext",
        json!({
            "session_id": "sess_plan",
            "turn_id": "turn_plan",
            "runtime_instance_id": "rt_plan",
            "input": {"task_id": "task_other", "run_id": "run_other"}
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["ok"], true);
    assert_eq!(body["tool"], "getContext");
    let text = body["result"]["text"].as_str().expect("plain text context");
    assert!(text.contains("pontia context: planning"));
    assert!(text.contains("Role: planner"));
    assert!(text.contains("Goal:"));
    assert!(!text.contains("sess_plan"));
    assert!(!text.contains("turn_plan"));
    assert!(!text.contains("rt_plan"));
}

#[tokio::test]
async fn authorizes_execution_context_from_current_work_item_run_not_request_input() {
    let state = test_state().await;
    insert_task(&state.db(), "task_exec").await;
    insert_task(&state.db(), "task_other").await;
    insert_dag_session(
        &state.db(),
        "sess_exec",
        "turn_exec",
        "rt_exec",
        json!({"dag_managed": true}),
    )
    .await;
    insert_execution_run(
        &state.db(),
        "task_exec",
        "wi_exec",
        "run_exec",
        "sess_exec",
        "turn_exec",
    )
    .await;

    let (status, body) = post_tool(
        state,
        "getContext",
        json!({
            "session_id": "sess_exec",
            "turn_id": "turn_exec",
            "runtime_instance_id": "rt_exec",
            "input": {"task_id": "task_other", "run_id": "run_other", "work_item_id": "wi_other"}
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let text = body["result"]["text"].as_str().expect("plain text context");
    assert!(text.contains("pontia context: execution"));
    assert!(text.contains("ID: wi_exec"));
    assert!(!text.contains("task_other"));
    assert!(!text.contains("run_other"));
    assert!(!text.contains("wi_other"));
}

#[tokio::test]
async fn get_context_returns_planning_view_from_authoritative_task_state() {
    let state = test_state().await;
    insert_task(&state.db(), "task_plan").await;
    insert_dag_session(
        &state.db(),
        "sess_plan",
        "turn_plan",
        "rt_plan",
        json!({
            "dag_managed": true,
            "dag_planning_role": "planner",
            "task_id": "task_plan"
        }),
    )
    .await;
    insert_work_item(
        &state.db(),
        "task_plan",
        "wi_plan",
        "Existing item",
        "ready",
        json!(["accept it"]),
    )
    .await;
    insert_signal(&state.db(), "sig_open", "task_plan", None, None, "open").await;
    insert_signal(
        &state.db(),
        "sig_resolved",
        "task_plan",
        None,
        None,
        "resolved",
    )
    .await;
    insert_proposal(&state.db(), "prop_pending", "task_plan", "proposed").await;

    let (status, body) = post_tool(
        state,
        "getContext",
        json!({
            "session_id": "sess_plan",
            "turn_id": "turn_plan",
            "runtime_instance_id": "rt_plan",
            "input": {"task_id": "task_other"}
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let result = &body["result"];
    let text = result["text"].as_str().expect("plain text context");
    assert!(text.contains("pontia context: planning"));
    assert!(text.contains("Role: planner"));
    assert!(text.contains("Goal:"));
    assert!(text.contains("Current DAG:"));
    assert!(text.contains("wi_plan [ready] Existing item"));
    assert!(text.contains("Open signals:"));
    assert!(text.contains("sig_open [medium / needs_input]"));
    assert!(text.contains("Relevant proposals:"));
    assert!(text.contains("prop_pending [proposed / initial_dag]"));
    assert!(text.contains("Available execution profiles:"));
    assert!(text.contains("implementer"));
    assert!(!text.contains("created_at"));
    assert!(!text.contains("updated_at"));
    assert!(!text.contains("metadata"));
    assert!(!text.contains("null"));
}

#[tokio::test]
async fn get_context_returns_execution_view_scoped_to_current_run() {
    let state = test_state().await;
    insert_task(&state.db(), "task_exec").await;
    insert_task(&state.db(), "task_other").await;
    insert_dag_session(
        &state.db(),
        "sess_exec",
        "turn_exec",
        "rt_exec",
        json!({"dag_managed": true}),
    )
    .await;
    insert_work_item(
        &state.db(),
        "task_exec",
        "wi_upstream",
        "Upstream item",
        "completed",
        json!([]),
    )
    .await;
    insert_execution_run(
        &state.db(),
        "task_exec",
        "wi_exec",
        "run_exec",
        "sess_exec",
        "turn_exec",
    )
    .await;
    insert_edge(&state.db(), "task_exec", "wi_upstream", "wi_exec").await;
    insert_work_item(
        &state.db(),
        "task_other",
        "wi_other",
        "Other task item",
        "ready",
        json!([]),
    )
    .await;
    insert_signal(
        &state.db(),
        "sig_run",
        "task_exec",
        Some("wi_exec"),
        Some("run_exec"),
        "open",
    )
    .await;
    insert_signal(&state.db(), "sig_other", "task_other", None, None, "open").await;

    let (status, body) = post_tool(
        state,
        "getContext",
        json!({
            "session_id": "sess_exec",
            "turn_id": "turn_exec",
            "runtime_instance_id": "rt_exec",
            "input": {"task_id": "task_other", "work_item_id": "wi_other"}
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let result = &body["result"];
    let text = result["text"].as_str().expect("plain text context");
    assert!(text.contains("pontia context: execution"));
    assert!(text.contains("Current WorkItem:"));
    assert!(text.contains("ID: wi_exec"));
    assert!(text.contains("Acceptance criteria:"));
    assert!(text.contains("done means done"));
    assert!(text.contains("Completed dependencies:"));
    assert!(text.contains("wi_upstream [completed] Upstream item"));
    assert!(text.contains("Open related signals:"));
    assert!(text.contains("sig_run [medium / needs_input]"));
    assert!(!text.contains("wi_other"));
    assert!(!text.contains("created_at"));
    assert!(!text.contains("updated_at"));
    assert!(!text.contains("metadata"));
    assert!(!text.contains("null"));
}
