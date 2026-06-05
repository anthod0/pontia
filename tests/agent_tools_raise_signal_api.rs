#![cfg(any())]

#[path = "support/agent_tools.rs"]
mod agent_tools;

use agent_tools::*;
use axum::http::StatusCode;
use pilotfy::application::SqliteDagGraphStore;
use serde_json::{Value, json};
use sqlx::Row;

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
    let signal_event_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM task_events WHERE task_id = 'task_signal' AND event_type = 'signal.emitted'",
    )
    .fetch_one(&state.db)
    .await
    .expect("signal event count");
    assert_eq!(signal_event_count, 1);
    let graph = SqliteDagGraphStore::new(state.db.clone())
        .task_graph("task_signal")
        .await
        .expect("task graph");
    assert_eq!(graph.signals.len(), 1);
    assert_eq!(graph.signals[0].summary, "Need one more step");

    let replanner_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sessions WHERE json_extract(metadata, '$.dag_planning_role') = 'replanner' AND json_extract(metadata, '$.task_id') = 'task_signal'",
    )
    .fetch_one(&state.db)
    .await
    .expect("replanner count");
    assert_eq!(replanner_count, 1);
    let run_state: String =
        sqlx::query_scalar("SELECT state FROM work_item_runs WHERE run_id = 'run_signal_worker'")
            .fetch_one(&state.db)
            .await
            .expect("run state");
    assert_eq!(run_state, "blocked");
    let runtime_state: String = sqlx::query_scalar(
        "SELECT current_state FROM work_item_runtime_projection WHERE work_item_id = 'wi_signal_worker'",
    )
    .fetch_one(&state.db)
    .await
    .expect("runtime state");
    assert_eq!(runtime_state, "replan_anchor");
    let worker_session_state: String =
        sqlx::query_scalar("SELECT state FROM sessions WHERE session_id = 'sess_signal_worker'")
            .fetch_one(&state.db)
            .await
            .expect("worker session state");
    assert_eq!(worker_session_state, "exited");

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
async fn raise_signal_for_needs_input_blocks_current_run_and_task() {
    let state = test_state().await;
    insert_task(&state.db, "task_needs_input_signal").await;
    insert_dag_session(
        &state.db,
        "sess_needs_input_worker",
        "turn_needs_input_worker",
        "rt_needs_input_worker",
        json!({"dag_managed": true}),
    )
    .await;
    insert_execution_run(
        &state.db,
        "task_needs_input_signal",
        "wi_needs_input_worker",
        "run_needs_input_worker",
        "sess_needs_input_worker",
        "turn_needs_input_worker",
    )
    .await;

    let (status, body) = post_tool(
        state.clone(),
        "raiseSignal",
        json!({
            "session_id": "sess_needs_input_worker",
            "turn_id": "turn_needs_input_worker",
            "runtime_instance_id": "rt_needs_input_worker",
            "input": {
                "kind": "needs_input",
                "summary": "Need user decision",
                "severity": "high"
            }
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body:#}");
    assert_eq!(body["result"]["policy"]["replanner_started"], false);
    let run_state: String = sqlx::query_scalar(
        "SELECT state FROM work_item_runs WHERE run_id = 'run_needs_input_worker'",
    )
    .fetch_one(&state.db)
    .await
    .expect("run state");
    assert_eq!(run_state, "needs_input");
    let (runtime_state, blocked_reason): (String, String) = sqlx::query_as(
        "SELECT current_state, blocked_reason FROM work_item_runtime_projection WHERE work_item_id = 'wi_needs_input_worker'",
    )
    .fetch_one(&state.db)
    .await
    .expect("runtime state");
    assert_eq!(runtime_state, "needs_input");
    assert_eq!(blocked_reason, "Need user decision");
    let task_state: String =
        sqlx::query_scalar("SELECT state FROM tasks WHERE task_id = 'task_needs_input_signal'")
            .fetch_one(&state.db)
            .await
            .expect("task state");
    assert_eq!(task_state, "blocked");
    let worker_session_state: String = sqlx::query_scalar(
        "SELECT state FROM sessions WHERE session_id = 'sess_needs_input_worker'",
    )
    .fetch_one(&state.db)
    .await
    .expect("worker session state");
    assert_eq!(worker_session_state, "exited");

    cleanup_runtime_sessions(&state.db).await;
}
