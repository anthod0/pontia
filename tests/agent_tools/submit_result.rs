#![cfg(any())]

#[path = "../support/agent_tools.rs"]
mod agent_tools;

use agent_tools::*;
use axum::http::StatusCode;
use serde_json::{Value, json};
use sqlx::Row;

#[tokio::test]
async fn submit_result_from_worker_updates_current_run_and_schedules_downstream_once() {
    let state = test_state().await;
    insert_task(&state.db(), "task_result").await;
    insert_dag_session(
        &state.db(),
        "sess_result",
        "turn_result",
        "rt_result",
        json!({"dag_managed": true}),
    )
    .await;
    insert_execution_run(
        &state.db(),
        "task_result",
        "wi_result_upstream",
        "run_result_upstream",
        "sess_result",
        "turn_result",
    )
    .await;
    insert_work_item(
        &state.db(),
        "task_result",
        "wi_result_downstream",
        "Downstream",
        "blocked",
        json!(["downstream done"]),
    )
    .await;
    insert_edge(
        &state.db(),
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
    .fetch_one(&state.db())
    .await
    .expect("run");
    assert_eq!(run.get::<String, _>("state"), "completed");
    assert_eq!(run.get::<String, _>("output_summary"), "upstream done");
    let downstream_runs: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM work_item_runs WHERE work_item_id = 'wi_result_downstream'",
    )
    .fetch_one(&state.db())
    .await
    .expect("downstream run count");
    assert_eq!(downstream_runs, 1);
    let event_payload: String = sqlx::query_scalar(
        "SELECT payload FROM task_events WHERE task_id = 'task_result' AND event_type = 'dag.run_completed' ORDER BY created_at DESC LIMIT 1",
    )
    .fetch_one(&state.db())
    .await
    .expect("run completed event");
    let event_payload: Value = serde_json::from_str(&event_payload).expect("event payload json");
    assert_eq!(event_payload["outputs"][0]["ref"], "file:/tmp/demo.txt");

    let (status, response) = post_tool(state.clone(), "submitResult", body).await;
    assert_eq!(status, StatusCode::CONFLICT, "{response:#}");
    let downstream_runs_after_duplicate: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM work_item_runs WHERE work_item_id = 'wi_result_downstream'",
    )
    .fetch_one(&state.db())
    .await
    .expect("downstream run count after duplicate");
    assert_eq!(downstream_runs_after_duplicate, 1);

    cleanup_runtime_sessions(&state.db()).await;
}

#[tokio::test]
async fn submit_result_dispatches_downstream_before_terminating_calling_session() {
    let state = test_state().await;
    insert_task(&state.db(), "task_result_order").await;
    insert_dag_session(
        &state.db(),
        "sess_result_order",
        "turn_result_order",
        "rt_result_order",
        json!({"dag_managed": true}),
    )
    .await;
    insert_execution_run(
        &state.db(),
        "task_result_order",
        "wi_result_order_upstream",
        "run_result_order_upstream",
        "sess_result_order",
        "turn_result_order",
    )
    .await;
    insert_work_item(
        &state.db(),
        "task_result_order",
        "wi_result_order_downstream",
        "Downstream",
        "blocked",
        json!(["downstream done"]),
    )
    .await;
    insert_edge(
        &state.db(),
        "task_result_order",
        "wi_result_order_upstream",
        "wi_result_order_downstream",
    )
    .await;

    let (status, response) = post_tool(
        state.clone(),
        "submitResult",
        json!({
            "session_id": "sess_result_order",
            "turn_id": "turn_result_order",
            "runtime_instance_id": "rt_result_order",
            "input": {"status": "completed", "summary": "upstream done"}
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{response:#}");
    let downstream_session_created_rowid: i64 = sqlx::query_scalar(
        r#"SELECT rowid FROM events
           WHERE event_type = 'session.created'
             AND payload LIKE '%wi_result_order_downstream%'
           ORDER BY rowid
           LIMIT 1"#,
    )
    .fetch_one(&state.db())
    .await
    .expect("downstream session.created event");
    let calling_session_exited_rowid: i64 = sqlx::query_scalar(
        r#"SELECT rowid FROM events
           WHERE session_id = 'sess_result_order'
             AND event_type = 'session.exited'
           ORDER BY rowid
           LIMIT 1"#,
    )
    .fetch_one(&state.db())
    .await
    .expect("calling session.exited event");

    assert!(
        downstream_session_created_rowid < calling_session_exited_rowid,
        "downstream dispatch must be committed before terminating the session that submitted the result"
    );

    cleanup_runtime_sessions(&state.db()).await;
}

#[tokio::test]
async fn submit_result_requires_execution_context_and_supported_status() {
    let state = test_state().await;
    insert_task(&state.db(), "task_result_modes").await;
    insert_dag_session(
        &state.db(),
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
        &state.db(),
        "sess_result_worker",
        "turn_result_worker",
        "rt_result_worker",
        json!({"dag_managed": true}),
    )
    .await;
    insert_execution_run(
        &state.db(),
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
