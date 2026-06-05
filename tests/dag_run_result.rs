#![cfg(any())]

use pilotfy::{
    application::{
        DagSchedulerService, DagService, EventIngestService, SqliteDagGraphStore,
        SubmitPlanPayload, WorkItemDraft, WorkItemEdgeDraft,
    },
    domain::{DomainEvent, EventSource, EventType},
    ids::{new_event_id, new_task_id},
    storage::sqlite::{connect_sqlite, run_migrations},
};
use serde_json::json;
use sqlx::{Row, SqlitePool};
use std::process::{Command, Stdio};

async fn test_pool() -> SqlitePool {
    let db = connect_sqlite("sqlite://:memory:").await.expect("connect");
    run_migrations(&db).await.expect("migrate");
    db
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

async fn insert_task(pool: &SqlitePool) -> String {
    let task_id = new_task_id().to_string();
    sqlx::query("INSERT INTO tasks (task_id, state, input) VALUES (?, 'running', 'test task')")
        .bind(&task_id)
        .execute(pool)
        .await
        .expect("insert task");
    task_id
}

fn draft(temp_id: &str, priority: i64) -> WorkItemDraft {
    WorkItemDraft {
        temp_id: Some(temp_id.to_string()),
        title: format!("{temp_id} title"),
        description: format!("{temp_id} description"),
        kind: "implementation".to_string(),
        action: "agent_turn".to_string(),
        execution_profile_id: "implementer".to_string(),
        execution_profile_version: None,
        priority,
        optional: false,
        parallelizable: true,
        acceptance_criteria: vec!["done".to_string()],
        metadata: json!({}),
    }
}

fn edge(from: &str, to: &str) -> WorkItemEdgeDraft {
    WorkItemEdgeDraft {
        from_work_item_id: from.to_string(),
        to_work_item_id: to.to_string(),
        edge_type: "depends_on".to_string(),
    }
}

fn initial_plan(
    work_items: Vec<WorkItemDraft>,
    edges: Vec<WorkItemEdgeDraft>,
) -> SubmitPlanPayload {
    SubmitPlanPayload {
        mode: "initial_dag".to_string(),
        summary: "initial plan".to_string(),
        work_items,
        edges,
        assumptions: vec![],
        risks: vec![],
    }
}

async fn schedule_first_run(pool: &SqlitePool, task_id: &str) -> (String, String, String) {
    let outcome = DagSchedulerService::new(pool.clone())
        .schedule_task(task_id)
        .await
        .expect("schedule");
    assert_eq!(outcome.dispatched_runs.len(), 1);
    let dispatched = &outcome.dispatched_runs[0];
    (
        dispatched.run_id.clone(),
        dispatched.session_id.clone(),
        dispatched.turn_id.clone(),
    )
}

async fn ingest_turn_completed(
    pool: &SqlitePool,
    session_id: &str,
    turn_id: &str,
    payload: serde_json::Value,
) {
    EventIngestService::new(pool.clone())
        .ingest_event(DomainEvent::new(
            new_event_id().to_string(),
            session_id.to_string(),
            Some(turn_id.to_string()),
            EventSource::AgentAdapter,
            "generic".to_string(),
            EventType::TurnCompleted,
            payload,
        ))
        .await
        .expect("ingest completed");
}

async fn ingest_turn_failed(pool: &SqlitePool, session_id: &str, turn_id: &str, message: &str) {
    EventIngestService::new(pool.clone())
        .ingest_event(DomainEvent::new(
            new_event_id().to_string(),
            session_id.to_string(),
            Some(turn_id.to_string()),
            EventSource::AgentAdapter,
            "generic".to_string(),
            EventType::TurnFailed,
            json!({"failure":{"message": message}}),
        ))
        .await
        .expect("ingest failed");
}

#[tokio::test]
async fn worker_completed_json_updates_run_projection_and_dispatches_downstream() {
    let pool = test_pool().await;
    let task_id = insert_task(&pool).await;
    DagService::new(pool.clone())
        .apply_initial_dag(
            &task_id,
            &initial_plan(
                vec![draft("design", 0), draft("impl", 0)],
                vec![edge("design", "impl")],
            ),
        )
        .await
        .expect("apply dag");
    let (run_id, session_id, turn_id) = schedule_first_run(&pool, &task_id).await;

    ingest_turn_completed(
        &pool,
        &session_id,
        &turn_id,
        json!({"output":{"status":"completed","summary":"design done"}}),
    )
    .await;

    let run = sqlx::query("SELECT state, output_summary FROM work_item_runs WHERE run_id = ?")
        .bind(&run_id)
        .fetch_one(&pool)
        .await
        .expect("run");
    assert_eq!(run.get::<String, _>("state"), "completed");
    assert_eq!(run.get::<String, _>("output_summary"), "design done");

    let impl_id = SqliteDagGraphStore::new(pool.clone())
        .task_graph(&task_id)
        .await
        .expect("task graph")
        .work_items
        .into_iter()
        .find(|work_item| work_item.title == "impl title")
        .expect("impl work item")
        .work_item_id;
    let downstream = sqlx::query(
        r#"SELECT current_state, current_run_id, turn_id
           FROM work_item_runtime_projection
           WHERE work_item_id = ?"#,
    )
    .bind(impl_id)
    .fetch_one(&pool)
    .await
    .expect("downstream");
    assert_eq!(downstream.get::<String, _>("current_state"), "running");
    assert!(
        downstream
            .get::<Option<String>, _>("current_run_id")
            .is_some()
    );
    assert!(downstream.get::<Option<String>, _>("turn_id").is_some());

    cleanup_runtime_sessions(&pool).await;
}

#[tokio::test]
async fn non_json_output_degrades_to_completed_with_raw_summary() {
    let pool = test_pool().await;
    let task_id = insert_task(&pool).await;
    DagService::new(pool.clone())
        .apply_initial_dag(&task_id, &initial_plan(vec![draft("impl", 0)], vec![]))
        .await
        .expect("apply dag");
    let (run_id, session_id, turn_id) = schedule_first_run(&pool, &task_id).await;

    ingest_turn_completed(
        &pool,
        &session_id,
        &turn_id,
        json!({"output":{"summary":"plain text result"}}),
    )
    .await;

    let row = sqlx::query("SELECT r.state, r.output_summary, p.current_state FROM work_item_runs r JOIN work_item_runtime_projection p ON p.current_run_id = r.run_id WHERE r.run_id = ?")
        .bind(&run_id)
        .fetch_one(&pool)
        .await
        .expect("run row");
    assert_eq!(row.get::<String, _>("state"), "completed");
    assert_eq!(row.get::<String, _>("output_summary"), "plain text result");
    assert_eq!(row.get::<String, _>("current_state"), "completed");

    cleanup_runtime_sessions(&pool).await;
}

#[tokio::test]
async fn worker_signal_is_recorded_in_dag_signals() {
    let pool = test_pool().await;
    let task_id = insert_task(&pool).await;
    DagService::new(pool.clone())
        .apply_initial_dag(&task_id, &initial_plan(vec![draft("impl", 0)], vec![]))
        .await
        .expect("apply dag");
    let (run_id, session_id, turn_id) = schedule_first_run(&pool, &task_id).await;

    ingest_turn_completed(
        &pool,
        &session_id,
        &turn_id,
        json!({"output":{"summary": json!({
            "status":"blocked",
            "summary":"missing API docs",
            "signals":[{"kind":"missing_dependency","summary":"API docs missing","detail":"Need current API documentation","severity":"high"}]
        }).to_string()}}),
    )
    .await;

    let signal = sqlx::query(
        "SELECT task_id, run_id, source_session_id, kind, summary, detail, severity, state FROM dag_signals WHERE task_id = ?",
    )
    .bind(&task_id)
    .fetch_one(&pool)
    .await
    .expect("signal");
    assert_eq!(signal.get::<String, _>("task_id"), task_id);
    assert_eq!(signal.get::<String, _>("run_id"), run_id);
    assert_eq!(signal.get::<String, _>("source_session_id"), session_id);
    assert_eq!(signal.get::<String, _>("kind"), "missing_dependency");
    assert_eq!(signal.get::<String, _>("summary"), "API docs missing");
    assert_eq!(
        signal.get::<String, _>("detail"),
        "Need current API documentation"
    );
    assert_eq!(signal.get::<String, _>("severity"), "high");
    assert_eq!(signal.get::<String, _>("state"), "open");

    cleanup_runtime_sessions(&pool).await;
}

#[tokio::test]
async fn replan_requested_signal_automatically_starts_replanner_turn() {
    let pool = test_pool().await;
    let task_id = insert_task(&pool).await;
    DagService::new(pool.clone())
        .apply_initial_dag(&task_id, &initial_plan(vec![draft("impl", 0)], vec![]))
        .await
        .expect("apply dag");
    let (_run_id, session_id, turn_id) = schedule_first_run(&pool, &task_id).await;
    sqlx::query(
        "UPDATE execution_profiles SET supported_client_types = '[\"generic\"]' WHERE profile_id = 'replanner' AND version = '1'",
    )
    .execute(&pool)
    .await
    .expect("enable generic replanner for test");

    ingest_turn_completed(
        &pool,
        &session_id,
        &turn_id,
        json!({"output":{"summary": json!({
            "status":"blocked",
            "summary":"need replanning",
            "signals":[{"kind":"replan_requested","summary":"split the work","detail":"Current work item is too broad","severity":"medium"}]
        }).to_string()}}),
    )
    .await;

    let replanner = sqlx::query(
        r#"SELECT s.session_id, s.current_turn_id, s.metadata, t.state AS task_state
           FROM sessions s CROSS JOIN tasks t
           WHERE t.task_id = ? AND json_extract(s.metadata, '$.dag_planning_role') = 'replanner'"#,
    )
    .bind(&task_id)
    .fetch_one(&pool)
    .await
    .expect("replanner session");
    assert!(
        replanner
            .get::<String, _>("session_id")
            .starts_with("sess_")
    );
    assert!(
        replanner
            .get::<Option<String>, _>("current_turn_id")
            .is_some()
    );
    assert_eq!(replanner.get::<String, _>("task_state"), "replanning");

    cleanup_runtime_sessions(&pool).await;
}

#[tokio::test]
async fn all_required_completed_marks_task_completed() {
    let pool = test_pool().await;
    let task_id = insert_task(&pool).await;
    DagService::new(pool.clone())
        .apply_initial_dag(&task_id, &initial_plan(vec![draft("impl", 0)], vec![]))
        .await
        .expect("apply dag");
    let (_run_id, session_id, turn_id) = schedule_first_run(&pool, &task_id).await;

    ingest_turn_completed(
        &pool,
        &session_id,
        &turn_id,
        json!({"output":{"summary": json!({"status":"completed","summary":"all done"}).to_string()}}),
    )
    .await;

    let state: String = sqlx::query_scalar("SELECT state FROM tasks WHERE task_id = ?")
        .bind(&task_id)
        .fetch_one(&pool)
        .await
        .expect("task state");
    assert_eq!(state, "completed");
    let session_state: String =
        sqlx::query_scalar("SELECT state FROM sessions WHERE session_id = ?")
            .bind(&session_id)
            .fetch_one(&pool)
            .await
            .expect("session state");
    assert_eq!(session_state, "exited");

    cleanup_runtime_sessions(&pool).await;
}

#[tokio::test]
async fn failed_and_blocked_results_map_to_task_failed_or_blocked() {
    let pool = test_pool().await;
    let failed_task_id = insert_task(&pool).await;
    DagService::new(pool.clone())
        .apply_initial_dag(
            &failed_task_id,
            &initial_plan(vec![draft("fail", 0)], vec![]),
        )
        .await
        .expect("apply failed dag");
    let (_run_id, session_id, turn_id) = schedule_first_run(&pool, &failed_task_id).await;
    ingest_turn_failed(&pool, &session_id, &turn_id, "worker crashed").await;
    let failed_state: String = sqlx::query_scalar("SELECT state FROM tasks WHERE task_id = ?")
        .bind(&failed_task_id)
        .fetch_one(&pool)
        .await
        .expect("failed task state");
    assert_eq!(failed_state, "failed");

    let blocked_task_id = insert_task(&pool).await;
    DagService::new(pool.clone())
        .apply_initial_dag(
            &blocked_task_id,
            &initial_plan(vec![draft("block", 0)], vec![]),
        )
        .await
        .expect("apply blocked dag");
    let (_run_id, session_id, turn_id) = schedule_first_run(&pool, &blocked_task_id).await;
    ingest_turn_completed(
        &pool,
        &session_id,
        &turn_id,
        json!({"output":{"status":"needs_input","summary":"need user choice"}}),
    )
    .await;
    let blocked_state: String = sqlx::query_scalar("SELECT state FROM tasks WHERE task_id = ?")
        .bind(&blocked_task_id)
        .fetch_one(&pool)
        .await
        .expect("blocked task state");
    assert_eq!(blocked_state, "blocked");

    cleanup_runtime_sessions(&pool).await;
}
