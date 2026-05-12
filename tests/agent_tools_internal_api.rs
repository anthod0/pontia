use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use llmparty::{
    application::AppState,
    config::AppConfig,
    storage::sqlite::{connect_sqlite, run_migrations},
    transport::http,
};
use serde_json::{Value, json};
use sqlx::{Row, SqlitePool};
use std::process::{Command, Stdio};
use tower::ServiceExt;

async fn test_state() -> AppState {
    let db = connect_sqlite("sqlite://:memory:").await.expect("connect");
    run_migrations(&db).await.expect("migrate");
    let config = AppConfig::from_vars(&std::collections::HashMap::new()).expect("default config");
    AppState {
        db,
        external_api_token: None,
        planner: config.planner,
        graph: config.graph,
        workspace_browser: config.workspace_browser,
    }
}

async fn post_tool(state: AppState, tool: &str, body: Value) -> (StatusCode, Value) {
    let response = http::router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/internal/v1/agent-tools/{tool}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
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

async fn insert_task(pool: &SqlitePool, task_id: &str) {
    sqlx::query("INSERT INTO tasks (task_id, state, input) VALUES (?, 'running', 'test task')")
        .bind(task_id)
        .execute(pool)
        .await
        .expect("insert task");
}

async fn cleanup_runtime_sessions(pool: &SqlitePool) {
    let refs: Vec<String> = sqlx::query_scalar("SELECT runtime_ref FROM runtime_bindings")
        .fetch_all(pool)
        .await
        .unwrap_or_default();
    for runtime_ref in refs {
        let _ = Command::new("tmux")
            .args(["kill-session", "-t", &runtime_ref])
            .stderr(Stdio::null())
            .status();
    }
}

async fn insert_dag_session(
    pool: &SqlitePool,
    session_id: &str,
    turn_id: &str,
    runtime_instance_id: &str,
    metadata: Value,
) {
    sqlx::query(
        r#"INSERT INTO sessions (session_id, client_type, state, current_turn_id, metadata)
           VALUES (?, 'pi', 'busy', ?, ?)"#,
    )
    .bind(session_id)
    .bind(turn_id)
    .bind(metadata.to_string())
    .execute(pool)
    .await
    .expect("insert session");
    sqlx::query(
        "INSERT INTO turns (turn_id, session_id, state, metadata) VALUES (?, ?, 'running', ?)",
    )
    .bind(turn_id)
    .bind(session_id)
    .bind(metadata.to_string())
    .execute(pool)
    .await
    .expect("insert turn");
    sqlx::query(
        "INSERT INTO runtime_bindings (session_id, runtime_kind, runtime_ref, metadata) VALUES (?, 'tmux', ?, ?)",
    )
    .bind(session_id)
    .bind(format!("rtref_{session_id}"))
    .bind(json!({"runtime_instance_id": runtime_instance_id}).to_string())
    .execute(pool)
    .await
    .expect("insert runtime binding");
}

async fn insert_work_item(
    pool: &SqlitePool,
    task_id: &str,
    work_item_id: &str,
    title: &str,
    state: &str,
    acceptance_criteria: Value,
) {
    sqlx::query(
        r#"INSERT INTO work_items (
                work_item_id, task_id, title, description, kind, action,
                execution_profile_id, acceptance_criteria
           ) VALUES (?, ?, ?, 'Do the current work', 'implementation', 'agent_turn', 'implementer', ?)"#,
    )
    .bind(work_item_id)
    .bind(task_id)
    .bind(title)
    .bind(acceptance_criteria.to_string())
    .execute(pool)
    .await
    .expect("insert work item");
    sqlx::query(
        r#"INSERT INTO work_item_runtime_projection (
                work_item_id, task_id, current_state, current_attempt, priority
           ) VALUES (?, ?, ?, 0, 0)"#,
    )
    .bind(work_item_id)
    .bind(task_id)
    .bind(state)
    .execute(pool)
    .await
    .expect("insert work item runtime");
}

async fn insert_execution_run(
    pool: &SqlitePool,
    task_id: &str,
    work_item_id: &str,
    run_id: &str,
    session_id: &str,
    turn_id: &str,
) {
    insert_work_item(
        pool,
        task_id,
        work_item_id,
        "Do work",
        "running",
        json!(["done means done"]),
    )
    .await;
    sqlx::query(
        r#"INSERT INTO work_item_runs (
                run_id, work_item_id, task_id, attempt, state, session_id, turn_id,
                client_type, execution_profile_id, rendered_prompt_ref
           ) VALUES (?, ?, ?, 1, 'running', ?, ?, 'pi', 'implementer', 'inline')"#,
    )
    .bind(run_id)
    .bind(work_item_id)
    .bind(task_id)
    .bind(session_id)
    .bind(turn_id)
    .execute(pool)
    .await
    .expect("insert run");
    sqlx::query(
        r#"UPDATE work_item_runtime_projection
           SET current_run_id = ?, current_attempt = 1, session_id = ?, turn_id = ?
           WHERE work_item_id = ?"#,
    )
    .bind(run_id)
    .bind(session_id)
    .bind(turn_id)
    .bind(work_item_id)
    .execute(pool)
    .await
    .expect("update work item runtime");
}

async fn insert_edge(pool: &SqlitePool, task_id: &str, from: &str, to: &str) {
    sqlx::query(
        "INSERT INTO work_item_edges (edge_id, task_id, from_work_item_id, to_work_item_id) VALUES (?, ?, ?, ?)",
    )
    .bind(format!("edge_{from}_{to}"))
    .bind(task_id)
    .bind(from)
    .bind(to)
    .execute(pool)
    .await
    .expect("insert edge");
}

async fn insert_signal(
    pool: &SqlitePool,
    signal_id: &str,
    task_id: &str,
    work_item_id: Option<&str>,
    run_id: Option<&str>,
    state: &str,
) {
    sqlx::query(
        r#"INSERT INTO dag_signals (
                signal_id, task_id, work_item_id, run_id, kind, summary, severity, state
           ) VALUES (?, ?, ?, ?, 'needs_input', 'Need input', 'medium', ?)"#,
    )
    .bind(signal_id)
    .bind(task_id)
    .bind(work_item_id)
    .bind(run_id)
    .bind(state)
    .execute(pool)
    .await
    .expect("insert signal");
}

async fn insert_proposal(pool: &SqlitePool, proposal_id: &str, task_id: &str, state: &str) {
    sqlx::query(
        r#"INSERT INTO dag_proposals (
                proposal_id, task_id, mode, state, summary, proposal_json, validation_json
           ) VALUES (?, ?, 'initial_dag', ?, 'Initial plan', '{"work_items":[]}', '{}')"#,
    )
    .bind(proposal_id)
    .bind(task_id)
    .bind(state)
    .execute(pool)
    .await
    .expect("insert proposal");
}

fn valid_initial_dag_input() -> Value {
    json!({
        "mode": "initial_dag",
        "summary": "Implement the task",
        "dag": {
            "work_items": [{
                "temp_id": "impl",
                "title": "Implement",
                "description": "Do the implementation",
                "kind": "implementation",
                "action": "agent_turn",
                "execution_profile_id": "implementer",
                "acceptance_criteria": ["done"]
            }],
            "edges": []
        },
        "assumptions": [],
        "risks": []
    })
}

fn valid_patch_input() -> Value {
    json!({
        "mode": "patch",
        "summary": "Add follow-up",
        "patch": {
            "operations": [{
                "op": "add_work_item",
                "work_item": {
                    "temp_id": "followup",
                    "title": "Follow up",
                    "description": "Do follow-up work",
                    "kind": "implementation",
                    "action": "agent_turn",
                    "execution_profile_id": "implementer",
                    "acceptance_criteria": ["follow-up done"]
                }
            }]
        }
    })
}

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
    let work_item_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM work_items WHERE task_id = ?")
            .bind("task_invalid_plan")
            .fetch_one(&state.db)
            .await
            .expect("work item count");
    assert_eq!(work_item_count, 0);
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
    let edge_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM work_item_edges WHERE task_id = ?")
            .bind("task_invalid_patch")
            .fetch_one(&state.db)
            .await
            .expect("edge count");
    assert_eq!(edge_count, 0);
}

#[tokio::test]
async fn submit_result_from_worker_updates_current_run_and_schedules_downstream_once() {
    let state = test_state().await;
    insert_task(&state.db, "task_result").await;
    insert_dag_session(
        &state.db,
        "sess_result",
        "turn_result",
        "rt_result",
        json!({"dag_managed": true}),
    )
    .await;
    insert_execution_run(
        &state.db,
        "task_result",
        "wi_result_upstream",
        "run_result_upstream",
        "sess_result",
        "turn_result",
    )
    .await;
    insert_work_item(
        &state.db,
        "task_result",
        "wi_result_downstream",
        "Downstream",
        "blocked",
        json!(["downstream done"]),
    )
    .await;
    insert_edge(
        &state.db,
        "task_result",
        "wi_result_upstream",
        "wi_result_downstream",
    )
    .await;

    let body = json!({
        "session_id": "sess_result",
        "turn_id": "turn_result",
        "runtime_instance_id": "rt_result",
        "input": {
            "status": "completed",
            "summary": "upstream done",
            "outputs": [{"kind":"document","name":"demo","ref":"file:/tmp/demo.txt"}],
            "failure": null,
            "run_id": "run_other"
        }
    });
    let (status, response) = post_tool(state.clone(), "submitResult", body.clone()).await;

    assert_eq!(status, StatusCode::OK, "{response:#}");
    assert_eq!(response["result"]["run_id"], "run_result_upstream");
    assert_eq!(response["result"]["state"], "completed");
    assert_eq!(
        response["result"]["scheduler"]["dispatched_runs"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let run = sqlx::query(
        "SELECT state, output_summary FROM work_item_runs WHERE run_id = 'run_result_upstream'",
    )
    .fetch_one(&state.db)
    .await
    .expect("run");
    assert_eq!(run.get::<String, _>("state"), "completed");
    assert_eq!(run.get::<String, _>("output_summary"), "upstream done");
    let downstream_runs: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM work_item_runs WHERE work_item_id = 'wi_result_downstream'",
    )
    .fetch_one(&state.db)
    .await
    .expect("downstream run count");
    assert_eq!(downstream_runs, 1);
    let event_payload: String = sqlx::query_scalar(
        "SELECT payload FROM task_events WHERE task_id = 'task_result' AND event_type = 'dag.run_completed' ORDER BY created_at DESC LIMIT 1",
    )
    .fetch_one(&state.db)
    .await
    .expect("run completed event");
    let event_payload: Value = serde_json::from_str(&event_payload).expect("event payload json");
    assert_eq!(event_payload["outputs"][0]["ref"], "file:/tmp/demo.txt");

    let (status, response) = post_tool(state.clone(), "submitResult", body).await;
    assert_eq!(status, StatusCode::CONFLICT, "{response:#}");
    let downstream_runs_after_duplicate: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM work_item_runs WHERE work_item_id = 'wi_result_downstream'",
    )
    .fetch_one(&state.db)
    .await
    .expect("downstream run count after duplicate");
    assert_eq!(downstream_runs_after_duplicate, 1);

    cleanup_runtime_sessions(&state.db).await;
}

#[tokio::test]
async fn submit_result_requires_execution_context_and_supported_status() {
    let state = test_state().await;
    insert_task(&state.db, "task_result_modes").await;
    insert_dag_session(
        &state.db,
        "sess_result_planner",
        "turn_result_planner",
        "rt_result_planner",
        json!({
            "dag_managed": true,
            "dag_planning_role": "planner",
            "task_id": "task_result_modes"
        }),
    )
    .await;
    insert_dag_session(
        &state.db,
        "sess_result_worker",
        "turn_result_worker",
        "rt_result_worker",
        json!({"dag_managed": true}),
    )
    .await;
    insert_execution_run(
        &state.db,
        "task_result_modes",
        "wi_result_worker",
        "run_result_worker",
        "sess_result_worker",
        "turn_result_worker",
    )
    .await;

    let (status, body) = post_tool(
        state.clone(),
        "submitResult",
        json!({
            "session_id": "sess_result_planner",
            "turn_id": "turn_result_planner",
            "runtime_instance_id": "rt_result_planner",
            "input": {"status":"completed", "summary":"not allowed"}
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("execution")
    );

    let (status, body) = post_tool(
        state,
        "submitResult",
        json!({
            "session_id": "sess_result_worker",
            "turn_id": "turn_result_worker",
            "runtime_instance_id": "rt_result_worker",
            "input": {"status":"cancelled", "summary":"bad status"}
        }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("status")
    );
}

#[tokio::test]
async fn raise_signal_records_agent_signal_and_replan_policy_starts_replanner() {
    let state = test_state().await;
    insert_task(&state.db, "task_signal").await;
    insert_dag_session(
        &state.db,
        "sess_signal_worker",
        "turn_signal_worker",
        "rt_signal_worker",
        json!({"dag_managed": true}),
    )
    .await;
    insert_execution_run(
        &state.db,
        "task_signal",
        "wi_signal_worker",
        "run_signal_worker",
        "sess_signal_worker",
        "turn_signal_worker",
    )
    .await;

    let (status, body) = post_tool(
        state.clone(),
        "raiseSignal",
        json!({
            "session_id": "sess_signal_worker",
            "turn_id": "turn_signal_worker",
            "runtime_instance_id": "rt_signal_worker",
            "input": {
                "kind": "replan_requested",
                "summary": "Need one more step",
                "detail": "Validation is missing",
                "severity": "medium",
                "related_refs": [{"type":"work_item","id":"wi_signal_worker"}]
            }
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body:#}");
    assert_eq!(body["result"]["kind"], "replan_requested");
    assert_eq!(body["result"]["work_item_id"], "wi_signal_worker");
    assert_eq!(body["result"]["run_id"], "run_signal_worker");
    assert_eq!(body["result"]["policy"]["replanner_started"], true);
    let signal = sqlx::query(
        "SELECT source_session_id, kind, summary, detail, severity, related_refs, state FROM dag_signals WHERE task_id = 'task_signal'",
    )
    .fetch_one(&state.db)
    .await
    .expect("signal");
    assert_eq!(
        signal.get::<String, _>("source_session_id"),
        "sess_signal_worker"
    );
    assert_eq!(signal.get::<String, _>("kind"), "replan_requested");
    assert_eq!(signal.get::<String, _>("summary"), "Need one more step");
    assert_eq!(signal.get::<String, _>("detail"), "Validation is missing");
    assert_eq!(signal.get::<String, _>("severity"), "medium");
    assert_eq!(signal.get::<String, _>("state"), "open");
    let related_refs: Value =
        serde_json::from_str(&signal.get::<String, _>("related_refs")).expect("related refs json");
    assert_eq!(related_refs[0]["id"], "wi_signal_worker");
    let replanner_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sessions WHERE json_extract(metadata, '$.dag_planning_role') = 'replanner' AND json_extract(metadata, '$.task_id') = 'task_signal'",
    )
    .fetch_one(&state.db)
    .await
    .expect("replanner count");
    assert_eq!(replanner_count, 1);

    cleanup_runtime_sessions(&state.db).await;
}

#[tokio::test]
async fn raise_signal_from_planning_turn_records_task_scoped_open_signal() {
    let state = test_state().await;
    insert_task(&state.db, "task_planning_signal").await;
    insert_dag_session(
        &state.db,
        "sess_planning_signal",
        "turn_planning_signal",
        "rt_planning_signal",
        json!({
            "dag_managed": true,
            "dag_planning_role": "planner",
            "task_id": "task_planning_signal"
        }),
    )
    .await;

    let (status, body) = post_tool(
        state.clone(),
        "raiseSignal",
        json!({
            "session_id": "sess_planning_signal",
            "turn_id": "turn_planning_signal",
            "runtime_instance_id": "rt_planning_signal",
            "input": {
                "kind": "risk",
                "summary": "Plan has uncertainty",
                "severity": "low"
            }
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body:#}");
    assert_eq!(body["result"]["task_id"], "task_planning_signal");
    assert!(body["result"].get("work_item_id").unwrap().is_null());
    assert_eq!(body["result"]["policy"]["replanner_started"], false);
    let row = sqlx::query(
        "SELECT work_item_id, run_id, source_session_id, kind, state FROM dag_signals WHERE task_id = 'task_planning_signal'",
    )
    .fetch_one(&state.db)
    .await
    .expect("planning signal");
    assert!(row.get::<Option<String>, _>("work_item_id").is_none());
    assert!(row.get::<Option<String>, _>("run_id").is_none());
    assert_eq!(
        row.get::<String, _>("source_session_id"),
        "sess_planning_signal"
    );
    assert_eq!(row.get::<String, _>("kind"), "risk");
    assert_eq!(row.get::<String, _>("state"), "open");
}

#[tokio::test]
async fn rejects_runtime_mismatch_and_non_dag_managed_turns() {
    let state = test_state().await;
    insert_task(&state.db, "task_plan").await;
    insert_dag_session(
        &state.db,
        "sess_plan",
        "turn_plan",
        "rt_expected",
        json!({
            "dag_managed": true,
            "dag_planning_role": "replanner",
            "task_id": "task_plan"
        }),
    )
    .await;

    let (status, body) = post_tool(
        state.clone(),
        "raiseSignal",
        json!({
            "session_id": "sess_plan",
            "turn_id": "turn_plan",
            "runtime_instance_id": "rt_wrong",
            "input": {}
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "state_conflict");

    insert_dag_session(
        &state.db,
        "sess_plain",
        "turn_plain",
        "rt_plain",
        json!({"task_id": "task_plan"}),
    )
    .await;
    let (status, body) = post_tool(
        state,
        "getContext",
        json!({
            "session_id": "sess_plain",
            "turn_id": "turn_plain",
            "runtime_instance_id": "rt_plain",
            "input": {}
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("DAG-managed")
    );
}
