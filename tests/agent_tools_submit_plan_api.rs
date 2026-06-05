#![cfg(any())]

#[path = "support/agent_tools.rs"]
mod agent_tools;

use agent_tools::*;
use axum::http::StatusCode;
use pilotfy::application::SqliteDagGraphStore;
use serde_json::json;

#[tokio::test]
async fn submit_plan_from_planner_saves_applies_and_schedules_initial_dag() {
    let state = test_state().await;
    insert_task(&state.db, "task_submit_plan").await;
    insert_dag_session(
        &state.db,
        "sess_planner_submit",
        "turn_planner_submit",
        "rt_planner_submit",
        json!({
            "dag_managed": true,
            "dag_planning_role": "planner",
            "task_id": "task_submit_plan"
        }),
    )
    .await;

    let (status, body) = post_tool(
        state.clone(),
        "submitPlan",
        json!({
            "session_id": "sess_planner_submit",
            "turn_id": "turn_planner_submit",
            "runtime_instance_id": "rt_planner_submit",
            "input": valid_initial_dag_input()
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body:#}");
    let result = &body["result"];
    assert!(
        result["proposal_id"]
            .as_str()
            .unwrap()
            .starts_with("dagprop_")
    );
    assert_eq!(result["validation"]["ok"], true);
    assert_eq!(result["apply"]["applied"], true);
    assert_eq!(result["apply"]["mode"], "initial_dag");
    assert_eq!(
        result["scheduler"]["dispatched_runs"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    let proposal_state: String = sqlx::query_scalar(
        "SELECT state FROM dag_proposals WHERE proposal_id = ? AND created_by_session_id = ?",
    )
    .bind(result["proposal_id"].as_str().unwrap())
    .bind("sess_planner_submit")
    .fetch_one(&state.db)
    .await
    .expect("proposal state");
    assert_eq!(proposal_state, "applied");
    let task_state: String = sqlx::query_scalar("SELECT state FROM tasks WHERE task_id = ?")
        .bind("task_submit_plan")
        .fetch_one(&state.db)
        .await
        .expect("task state");
    assert_eq!(task_state, "running");
    let planner_session_state: String =
        sqlx::query_scalar("SELECT state FROM sessions WHERE session_id = ?")
            .bind("sess_planner_submit")
            .fetch_one(&state.db)
            .await
            .expect("planner session state");
    assert_eq!(planner_session_state, "exited");

    cleanup_runtime_sessions(&state.db).await;
}

#[tokio::test]
async fn submit_plan_enforces_planner_replanner_and_worker_modes() {
    let state = test_state().await;
    insert_task(&state.db, "task_submit_modes").await;
    insert_dag_session(
        &state.db,
        "sess_planner_modes",
        "turn_planner_modes",
        "rt_planner_modes",
        json!({
            "dag_managed": true,
            "dag_planning_role": "planner",
            "task_id": "task_submit_modes"
        }),
    )
    .await;
    insert_dag_session(
        &state.db,
        "sess_replanner_modes",
        "turn_replanner_modes",
        "rt_replanner_modes",
        json!({
            "dag_managed": true,
            "dag_planning_role": "replanner",
            "task_id": "task_submit_modes"
        }),
    )
    .await;
    insert_dag_session(
        &state.db,
        "sess_worker_modes",
        "turn_worker_modes",
        "rt_worker_modes",
        json!({"dag_managed": true}),
    )
    .await;
    insert_execution_run(
        &state.db,
        "task_submit_modes",
        "wi_worker_modes",
        "run_worker_modes",
        "sess_worker_modes",
        "turn_worker_modes",
    )
    .await;

    let (status, body) = post_tool(
        state.clone(),
        "submitPlan",
        json!({
            "session_id": "sess_planner_modes",
            "turn_id": "turn_planner_modes",
            "runtime_instance_id": "rt_planner_modes",
            "input": valid_patch_input()
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Planner")
    );

    let (status, body) = post_tool(
        state.clone(),
        "submitPlan",
        json!({
            "session_id": "sess_replanner_modes",
            "turn_id": "turn_replanner_modes",
            "runtime_instance_id": "rt_replanner_modes",
            "input": valid_initial_dag_input()
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("RePlanner")
    );

    let (status, body) = post_tool(
        state,
        "submitPlan",
        json!({
            "session_id": "sess_worker_modes",
            "turn_id": "turn_worker_modes",
            "runtime_instance_id": "rt_worker_modes",
            "input": valid_initial_dag_input()
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("planning turn")
    );
}

#[tokio::test]
async fn submit_plan_rejects_malformed_work_item_as_bad_request() {
    let state = test_state().await;
    insert_task(&state.db, "task_malformed_plan").await;
    insert_dag_session(
        &state.db,
        "sess_malformed_plan",
        "turn_malformed_plan",
        "rt_malformed_plan",
        json!({
            "dag_managed": true,
            "dag_planning_role": "planner",
            "task_id": "task_malformed_plan"
        }),
    )
    .await;

    let mut input = valid_initial_dag_input();
    input["dag"]["work_items"][0]
        .as_object_mut()
        .unwrap()
        .remove("kind");

    let (status, body) = post_tool(
        state,
        "submitPlan",
        json!({
            "session_id": "sess_malformed_plan",
            "turn_id": "turn_malformed_plan",
            "runtime_instance_id": "rt_malformed_plan",
            "input": input
        }),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "{body:#}");
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("invalid submitPlan dag.work_items")
    );
}

#[tokio::test]
async fn submit_plan_accepts_structured_risks_from_tool_contract() {
    let state = test_state().await;
    insert_task(&state.db, "task_structured_risks").await;
    insert_dag_session(
        &state.db,
        "sess_structured_risks",
        "turn_structured_risks",
        "rt_structured_risks",
        json!({
            "dag_managed": true,
            "dag_planning_role": "planner",
            "task_id": "task_structured_risks"
        }),
    )
    .await;

    let mut input = valid_initial_dag_input();
    input["risks"] = json!([{ "summary": "May need fixture updates", "severity": "low" }]);

    let (status, body) = post_tool(
        state.clone(),
        "submitPlan",
        json!({
            "session_id": "sess_structured_risks",
            "turn_id": "turn_structured_risks",
            "runtime_instance_id": "rt_structured_risks",
            "input": input
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body:#}");
    let proposal_json: String = sqlx::query_scalar(
        "SELECT proposal_json FROM dag_proposals WHERE task_id = ? AND created_by_session_id = ?",
    )
    .bind("task_structured_risks")
    .bind("sess_structured_risks")
    .fetch_one(&state.db)
    .await
    .expect("proposal json");
    let proposal: serde_json::Value = serde_json::from_str(&proposal_json).unwrap();
    assert_eq!(proposal["risks"][0]["summary"], "May need fixture updates");

    cleanup_runtime_sessions(&state.db).await;
}

#[tokio::test]
async fn submit_plan_rejects_invalid_dag_without_partial_apply() {
    let state = test_state().await;
    insert_task(&state.db, "task_invalid_plan").await;
    insert_dag_session(
        &state.db,
        "sess_invalid_plan",
        "turn_invalid_plan",
        "rt_invalid_plan",
        json!({
            "dag_managed": true,
            "dag_planning_role": "planner",
            "task_id": "task_invalid_plan"
        }),
    )
    .await;

    let mut input = valid_initial_dag_input();
    input["dag"]["edges"] = json!([{ "from_work_item_id": "impl", "to_work_item_id": "missing" }]);

    let (status, body) = post_tool(
        state.clone(),
        "submitPlan",
        json!({
            "session_id": "sess_invalid_plan",
            "turn_id": "turn_invalid_plan",
            "runtime_instance_id": "rt_invalid_plan",
            "input": input
        }),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "{body:#}");
    let proposal_state: String = sqlx::query_scalar(
        "SELECT state FROM dag_proposals WHERE task_id = ? AND created_by_session_id = ?",
    )
    .bind("task_invalid_plan")
    .bind("sess_invalid_plan")
    .fetch_one(&state.db)
    .await
    .expect("rejected proposal");
    assert_eq!(proposal_state, "rejected");
    let graph = SqliteDagGraphStore::new(state.db.clone())
        .task_graph("task_invalid_plan")
        .await
        .expect("task graph");
    assert_eq!(graph.work_items.len(), 0);
}

#[tokio::test]
async fn submit_plan_rejects_invalid_patch_without_partial_apply() {
    let state = test_state().await;
    insert_task(&state.db, "task_invalid_patch").await;
    insert_dag_session(
        &state.db,
        "sess_invalid_patch",
        "turn_invalid_patch",
        "rt_invalid_patch",
        json!({
            "dag_managed": true,
            "dag_planning_role": "replanner",
            "task_id": "task_invalid_patch"
        }),
    )
    .await;

    let input = json!({
        "mode": "patch",
        "summary": "Invalid edge",
        "patch": {
            "operations": [{
                "op": "add_edge",
                "edge": {"from_work_item_id": "missing_from", "to_work_item_id": "missing_to"}
            }]
        }
    });

    let (status, body) = post_tool(
        state.clone(),
        "submitPlan",
        json!({
            "session_id": "sess_invalid_patch",
            "turn_id": "turn_invalid_patch",
            "runtime_instance_id": "rt_invalid_patch",
            "input": input
        }),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "{body:#}");
    let proposal_state: String = sqlx::query_scalar(
        "SELECT state FROM dag_proposals WHERE task_id = ? AND created_by_session_id = ?",
    )
    .bind("task_invalid_patch")
    .bind("sess_invalid_patch")
    .fetch_one(&state.db)
    .await
    .expect("rejected patch proposal");
    assert_eq!(proposal_state, "rejected");
    let graph = SqliteDagGraphStore::new(state.db.clone())
        .task_graph("task_invalid_patch")
        .await
        .expect("task graph");
    assert_eq!(graph.edges.len(), 0);
}
