use crate::agent_tools_support::*;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use pontia_http as http;
use serde_json::{Value, json};
use tower::ServiceExt;

fn isolate_graph(
    state: pontia_application::AppState,
    dir: &tempfile::TempDir,
) -> pontia_application::AppState {
    let mut graph = state.graph();
    graph.enabled = true;
    graph.db_dir = Some(dir.path().join("lbug").display().to_string());
    state.with_graph(graph)
}

async fn get_external(state: pontia_application::AppState, uri: &str) -> (StatusCode, Value) {
    let response = http::router(state)
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(uri)
                .header("Authorization", "Bearer test-token")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    let status = response.status();
    let body = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    let json = serde_json::from_slice(&body).expect("json body");
    (status, json)
}

#[tokio::test]
async fn submit_plan_only_saves_pending_proposal_until_apply_plan_runs() {
    let graph_dir = tempfile::tempdir().expect("graph dir");
    let state = isolate_graph(test_state().await, &graph_dir);
    insert_task(&state.db(), "task_submit_pending").await;
    insert_dag_session(
        &state.db(),
        "sess_submit_pending",
        "turn_submit_pending",
        "rt_submit_pending",
        json!({
            "dag_managed": true,
            "dag_planning_role": "planner",
            "task_id": "task_submit_pending"
        }),
    )
    .await;

    let (status, body) = post_tool(
        state.clone(),
        "submitPlan",
        json!({
            "session_id": "sess_submit_pending",
            "turn_id": "turn_submit_pending",
            "runtime_instance_id": "rt_submit_pending",
            "input": valid_initial_dag_input()
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body:#}");
    let result = &body["result"];
    let proposal_id = result["proposal_id"].as_str().unwrap();
    assert_eq!(result["validation"]["ok"], true);
    assert_eq!(result["apply"]["applied"], false);
    assert_eq!(result["apply"]["proposal_state"], "proposed");
    assert_eq!(
        result["scheduler"]["dispatched_runs"]
            .as_array()
            .unwrap()
            .len(),
        0
    );

    let row: (String, i64) = sqlx::query_as(
        "SELECT state, revision FROM dag_proposals WHERE proposal_id = ? AND created_by_session_id = ?",
    )
    .bind(proposal_id)
    .bind("sess_submit_pending")
    .fetch_one(&state.db())
    .await
    .expect("proposal row");
    assert_eq!(row, ("proposed".to_string(), 1));

    let task_state: String = sqlx::query_scalar("SELECT state FROM tasks WHERE task_id = ?")
        .bind("task_submit_pending")
        .fetch_one(&state.db())
        .await
        .expect("task state");
    assert_eq!(task_state, "awaiting_approval");

    let planner_session_state: String =
        sqlx::query_scalar("SELECT state FROM sessions WHERE session_id = ?")
            .bind("sess_submit_pending")
            .fetch_one(&state.db())
            .await
            .expect("planner session state");
    assert_eq!(planner_session_state, "busy");

    let work_item_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM work_item_runtime_projection WHERE task_id = ?")
            .bind("task_submit_pending")
            .fetch_one(&state.db())
            .await
            .expect("work item count");
    assert_eq!(work_item_count, 0);

    cleanup_runtime_sessions(&state.db()).await;
}

#[tokio::test]
async fn apply_plan_applies_proposed_plan_and_starts_scheduler() {
    let graph_dir = tempfile::tempdir().expect("graph dir");
    let state = isolate_graph(test_state().await, &graph_dir);
    insert_task(&state.db(), "task_apply_plan").await;
    insert_dag_session_with_client(
        &state.db(),
        "sess_apply_plan",
        "turn_apply_plan",
        "rt_apply_plan",
        "generic",
        json!({
            "dag_managed": true,
            "dag_planning_role": "planner",
            "task_id": "task_apply_plan"
        }),
    )
    .await;

    let (_, submit_body) = post_tool(
        state.clone(),
        "submitPlan",
        json!({
            "session_id": "sess_apply_plan",
            "turn_id": "turn_apply_plan",
            "runtime_instance_id": "rt_apply_plan",
            "input": valid_initial_dag_input()
        }),
    )
    .await;
    let proposal_id = submit_body["result"]["proposal_id"].as_str().unwrap();

    let (status, body) = post_tool(
        state.clone(),
        "applyPlan",
        json!({
            "session_id": "sess_apply_plan",
            "turn_id": "turn_apply_plan",
            "runtime_instance_id": "rt_apply_plan",
            "input": {
                "proposal_id": proposal_id,
                "approval_quote": "同意，开始执行"
            }
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body:#}");
    let result = &body["result"];
    assert_eq!(result["proposal_id"], proposal_id);
    assert_eq!(result["apply"]["applied"], true);
    assert_eq!(result["apply"]["proposal_state"], "applied");
    let dispatched_runs = result["scheduler"]["dispatched_runs"]
        .as_array()
        .expect("dispatched runs");
    assert_eq!(dispatched_runs.len(), 1);
    let executor_session_id = dispatched_runs[0]["session_id"]
        .as_str()
        .expect("executor session id");
    let runtime_metadata: String =
        sqlx::query_scalar("SELECT metadata FROM runtime_bindings WHERE session_id = ?")
            .bind(executor_session_id)
            .fetch_one(&state.db())
            .await
            .expect("executor runtime metadata");
    let runtime_metadata: Value =
        serde_json::from_str(&runtime_metadata).expect("runtime metadata json");
    let runtime_dir = runtime_metadata["runtime_dir"]
        .as_str()
        .expect("runtime dir");
    assert!(
        !std::path::Path::new(runtime_dir)
            .join("runtime.sh")
            .exists(),
        "runtime.sh must not be a stable runtime artifact"
    );
    let start_command = runtime_metadata["start_command"]
        .as_str()
        .expect("start command");
    assert!(
        start_command.contains("cat >>"),
        "agent-tools tests must use a harmless pi runtime stub, got:\n{start_command}"
    );
    assert!(
        !start_command.contains("pi --approve --session-id"),
        "agent-tools tests must not launch the real pi runtime:\n{start_command}"
    );

    let proposal_state: String =
        sqlx::query_scalar("SELECT state FROM dag_proposals WHERE proposal_id = ?")
            .bind(proposal_id)
            .fetch_one(&state.db())
            .await
            .expect("proposal state");
    assert_eq!(proposal_state, "applied");

    let task_state: String = sqlx::query_scalar("SELECT state FROM tasks WHERE task_id = ?")
        .bind("task_apply_plan")
        .fetch_one(&state.db())
        .await
        .expect("task state");
    assert_eq!(task_state, "running");

    let work_item_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM work_item_runtime_projection WHERE task_id = ?")
            .bind("task_apply_plan")
            .fetch_one(&state.db())
            .await
            .expect("work item count");
    assert_eq!(work_item_count, 1);

    cleanup_runtime_sessions(&state.db()).await;
}

#[tokio::test]
async fn submit_plan_creates_revisions_and_supersedes_previous_pending_proposal() {
    let graph_dir = tempfile::tempdir().expect("graph dir");
    let state = isolate_graph(test_state().await, &graph_dir);
    insert_task(&state.db(), "task_plan_revisions").await;
    insert_dag_session(
        &state.db(),
        "sess_plan_revisions",
        "turn_plan_revisions",
        "rt_plan_revisions",
        json!({
            "dag_managed": true,
            "dag_planning_role": "planner",
            "task_id": "task_plan_revisions"
        }),
    )
    .await;

    let (_, first_body) = post_tool(
        state.clone(),
        "submitPlan",
        json!({
            "session_id": "sess_plan_revisions",
            "turn_id": "turn_plan_revisions",
            "runtime_instance_id": "rt_plan_revisions",
            "input": valid_initial_dag_input()
        }),
    )
    .await;
    let first_id = first_body["result"]["proposal_id"]
        .as_str()
        .unwrap()
        .to_string();

    let mut second_input = valid_initial_dag_input();
    second_input["summary"] = json!("Revised implementation plan");
    let (status, second_body) = post_tool(
        state.clone(),
        "submitPlan",
        json!({
            "session_id": "sess_plan_revisions",
            "turn_id": "turn_plan_revisions",
            "runtime_instance_id": "rt_plan_revisions",
            "input": second_input
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{second_body:#}");
    let second_id = second_body["result"]["proposal_id"].as_str().unwrap();

    let rows: Vec<(String, String, i64, Option<String>)> = sqlx::query_as(
        "SELECT proposal_id, state, revision, supersedes_proposal_id FROM dag_proposals WHERE task_id = ? ORDER BY revision ASC",
    )
    .bind("task_plan_revisions")
    .fetch_all(&state.db())
    .await
    .expect("proposal rows");

    assert_eq!(rows.len(), 2);
    assert_eq!(
        rows[0],
        (first_id.clone(), "superseded".to_string(), 1, None)
    );
    assert_eq!(
        rows[1],
        (
            second_id.to_string(),
            "proposed".to_string(),
            2,
            Some(first_id.clone())
        )
    );

    let (status, body) = post_tool(
        state.clone(),
        "applyPlan",
        json!({
            "session_id": "sess_plan_revisions",
            "turn_id": "turn_plan_revisions",
            "runtime_instance_id": "rt_plan_revisions",
            "input": { "proposal_id": first_id, "approval_quote": "approve old" }
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body:#}");
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("not proposed")
    );

    cleanup_runtime_sessions(&state.db()).await;
}

#[tokio::test]
async fn list_task_proposals_returns_all_revisions_with_full_body() {
    let graph_dir = tempfile::tempdir().expect("graph dir");
    let state = isolate_graph(
        test_state()
            .await
            .with_external_api_token(Some("test-token".to_string())),
        &graph_dir,
    );
    insert_task(&state.db(), "task_list_proposals").await;
    insert_dag_session(
        &state.db(),
        "sess_list_proposals",
        "turn_list_proposals",
        "rt_list_proposals",
        json!({
            "dag_managed": true,
            "dag_planning_role": "planner",
            "task_id": "task_list_proposals"
        }),
    )
    .await;

    let (_, first_body) = post_tool(
        state.clone(),
        "submitPlan",
        json!({
            "session_id": "sess_list_proposals",
            "turn_id": "turn_list_proposals",
            "runtime_instance_id": "rt_list_proposals",
            "input": valid_initial_dag_input()
        }),
    )
    .await;
    let first_id = first_body["result"]["proposal_id"]
        .as_str()
        .expect("first proposal id")
        .to_string();

    let mut second_input = valid_initial_dag_input();
    second_input["summary"] = json!("Revised proposal visible to dashboard");
    let (_, second_body) = post_tool(
        state.clone(),
        "submitPlan",
        json!({
            "session_id": "sess_list_proposals",
            "turn_id": "turn_list_proposals",
            "runtime_instance_id": "rt_list_proposals",
            "input": second_input
        }),
    )
    .await;
    let second_id = second_body["result"]["proposal_id"]
        .as_str()
        .expect("second proposal id");

    let (status, body) = get_external(
        state.clone(),
        "/external/v1/tasks/task_list_proposals/proposals",
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body:#}");
    let proposals = body["data"]["proposals"].as_array().expect("proposals");
    assert_eq!(proposals.len(), 2);

    assert_eq!(proposals[0]["proposal_id"], second_id);
    assert_eq!(proposals[0]["task_id"], "task_list_proposals");
    assert_eq!(proposals[0]["mode"], "initial_dag");
    assert_eq!(proposals[0]["state"], "proposed");
    assert_eq!(
        proposals[0]["summary"],
        "Revised proposal visible to dashboard"
    );
    assert_eq!(proposals[0]["revision"], 2);
    assert_eq!(proposals[0]["supersedes_proposal_id"], first_id);
    assert_eq!(proposals[0]["created_by_session_id"], "sess_list_proposals");
    assert_eq!(proposals[0]["created_by_turn_id"], "turn_list_proposals");
    assert_eq!(
        proposals[0]["proposal_json"]["summary"],
        "Revised proposal visible to dashboard"
    );
    assert_eq!(
        proposals[0]["proposal_json"]["work_items"][0]["temp_id"],
        "impl"
    );
    assert!(proposals[0]["validation_json"].as_object().is_some());
    assert!(proposals[0]["created_at"].as_str().is_some());
    assert!(proposals[0]["updated_at"].as_str().is_some());

    assert_eq!(proposals[1]["proposal_id"], first_id);
    assert_eq!(proposals[1]["state"], "superseded");
    assert_eq!(proposals[1]["revision"], 1);
    assert!(proposals[1]["supersedes_proposal_id"].is_null());

    let (missing_status, missing_body) =
        get_external(state.clone(), "/external/v1/tasks/missing_task/proposals").await;
    assert_eq!(missing_status, StatusCode::NOT_FOUND, "{missing_body:#}");

    cleanup_runtime_sessions(&state.db()).await;
}
