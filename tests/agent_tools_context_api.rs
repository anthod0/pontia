#[path = "support/agent_tools.rs"]
mod agent_tools;

use agent_tools::*;
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
    insert_task(&state.db, "task_plan").await;
    insert_dag_session(
        &state.db,
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
    let context = &body["result"]["context"];
    assert_eq!(context["session_id"], "sess_plan");
    assert_eq!(context["turn_id"], "turn_plan");
    assert_eq!(context["client_type"], "pi");
    assert_eq!(context["runtime_instance_id"], "rt_plan");
    assert_eq!(context["task_id"], "task_plan");
    assert_eq!(context["mode"]["type"], "planning");
    assert_eq!(context["mode"]["role"], "planner");
}

#[tokio::test]
async fn authorizes_execution_context_from_current_work_item_run_not_request_input() {
    let state = test_state().await;
    insert_task(&state.db, "task_exec").await;
    insert_task(&state.db, "task_other").await;
    insert_dag_session(
        &state.db,
        "sess_exec",
        "turn_exec",
        "rt_exec",
        json!({"dag_managed": true}),
    )
    .await;
    insert_execution_run(
        &state.db,
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
    let context = &body["result"]["context"];
    assert_eq!(context["task_id"], "task_exec");
    assert_eq!(context["mode"]["type"], "execution");
    assert_eq!(context["mode"]["run_id"], "run_exec");
    assert_eq!(context["mode"]["work_item_id"], "wi_exec");
}

#[tokio::test]
async fn get_context_returns_planning_view_from_authoritative_task_state() {
    let state = test_state().await;
    insert_task(&state.db, "task_plan").await;
    insert_dag_session(
        &state.db,
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
        &state.db,
        "task_plan",
        "wi_plan",
        "Existing item",
        "ready",
        json!(["accept it"]),
    )
    .await;
    insert_signal(&state.db, "sig_open", "task_plan", None, None, "open").await;
    insert_signal(
        &state.db,
        "sig_resolved",
        "task_plan",
        None,
        None,
        "resolved",
    )
    .await;
    insert_proposal(&state.db, "prop_pending", "task_plan", "proposed").await;

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
    assert_eq!(result["mode"], "planning");
    assert_eq!(result["role"], "planner");
    assert_eq!(result["task"]["task_id"], "task_plan");
    assert_eq!(result["dag"]["summary"]["total_work_items"], 1);
    assert_eq!(result["dag"]["work_items"][0]["work_item_id"], "wi_plan");
    assert_eq!(result["open_signals"].as_array().unwrap().len(), 1);
    assert_eq!(result["open_signals"][0]["signal_id"], "sig_open");
    assert_eq!(result["relevant_proposals"].as_array().unwrap().len(), 1);
    assert_eq!(
        result["relevant_proposals"][0]["proposal_id"],
        "prop_pending"
    );
    assert!(
        result["execution_profiles"]
            .as_array()
            .unwrap()
            .iter()
            .any(|profile| profile["profile_id"] == "implementer")
    );
    assert!(result.get("runtime_diagnostics").is_none());
}

#[tokio::test]
async fn get_context_returns_execution_view_scoped_to_current_run() {
    let state = test_state().await;
    insert_task(&state.db, "task_exec").await;
    insert_task(&state.db, "task_other").await;
    insert_dag_session(
        &state.db,
        "sess_exec",
        "turn_exec",
        "rt_exec",
        json!({"dag_managed": true}),
    )
    .await;
    insert_work_item(
        &state.db,
        "task_exec",
        "wi_upstream",
        "Upstream item",
        "completed",
        json!([]),
    )
    .await;
    insert_execution_run(
        &state.db,
        "task_exec",
        "wi_exec",
        "run_exec",
        "sess_exec",
        "turn_exec",
    )
    .await;
    insert_edge(&state.db, "task_exec", "wi_upstream", "wi_exec").await;
    insert_work_item(
        &state.db,
        "task_other",
        "wi_other",
        "Other task item",
        "ready",
        json!([]),
    )
    .await;
    insert_signal(
        &state.db,
        "sig_run",
        "task_exec",
        Some("wi_exec"),
        Some("run_exec"),
        "open",
    )
    .await;
    insert_signal(&state.db, "sig_other", "task_other", None, None, "open").await;

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
    assert_eq!(result["mode"], "execution");
    assert_eq!(result["task"]["task_id"], "task_exec");
    assert_eq!(result["work_item"]["work_item_id"], "wi_exec");
    assert_eq!(result["work_item_run"]["run_id"], "run_exec");
    assert_eq!(result["acceptance_criteria"], json!(["done means done"]));
    assert_eq!(result["dependencies"].as_array().unwrap().len(), 1);
    assert_eq!(
        result["dependencies"][0]["from_work_item_id"],
        "wi_upstream"
    );
    assert_eq!(
        result["upstream_completed_items"].as_array().unwrap().len(),
        1
    );
    assert_eq!(
        result["upstream_completed_items"][0]["work_item_id"],
        "wi_upstream"
    );
    assert_eq!(result["open_signals"].as_array().unwrap().len(), 1);
    assert_eq!(result["open_signals"][0]["signal_id"], "sig_run");
    assert!(!serde_json::to_string(result).unwrap().contains("wi_other"));
    assert!(result.get("runtime_diagnostics").is_none());
}
