#[path = "support/generic_client.rs"]
mod generic_client;

use generic_client::GenericClientTestScope;
use llmparty::{
    application::{
        DagSchedulerService, DagService, SubmitPlanPayload, WorkItemDraft, WorkItemEdgeDraft,
    },
    ids::new_task_id,
    storage::sqlite::{connect_sqlite, run_migrations},
};
use serde_json::{Value, json};
use sqlx::{Row, SqlitePool};
use std::process::Command;

async fn test_pool() -> SqlitePool {
    let db = connect_sqlite("sqlite://:memory:").await.expect("connect");
    run_migrations(&db).await.expect("migrate");
    db
}

async fn cleanup_scheduler_tmux_sessions(pool: &SqlitePool) {
    let refs: Vec<String> = sqlx::query_scalar("SELECT runtime_ref FROM runtime_bindings")
        .fetch_all(pool)
        .await
        .unwrap_or_default();
    for runtime_ref in refs {
        if runtime_ref.starts_with("generic:") {
            continue;
        }
        let _ = Command::new("tmux")
            .args(["kill-session", "-t", &runtime_ref])
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

async fn insert_dag_task_with_planner_client(pool: &SqlitePool, client_type: &str) -> String {
    let task_id = new_task_id().to_string();
    sqlx::query(
        r#"INSERT INTO tasks (task_id, state, input, metadata)
           VALUES (?, 'running', 'test task', ?)"#,
    )
    .bind(&task_id)
    .bind(json!({"dag_managed": true, "planner_client_type": client_type}).to_string())
    .execute(pool)
    .await
    .expect("insert dag task");
    task_id
}

fn draft(temp_id: &str, profile: &str, priority: i64) -> WorkItemDraft {
    WorkItemDraft {
        temp_id: Some(temp_id.to_string()),
        title: format!("{temp_id} title"),
        description: format!("{temp_id} description"),
        kind: "implementation".to_string(),
        action: "agent_turn".to_string(),
        execution_profile_id: profile.to_string(),
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

#[tokio::test]
async fn scheduler_dispatches_one_ready_work_item_as_run_session_and_turn() {
    let pool = test_pool().await;
    let task_id = insert_task(&pool).await;
    DagService::new(pool.clone())
        .apply_initial_dag(
            &task_id,
            &initial_plan(vec![draft("impl", "implementer", 10)], vec![]),
        )
        .await
        .expect("apply dag");

    let outcome = DagSchedulerService::new(pool.clone())
        .schedule_task(&task_id)
        .await
        .expect("schedule task");

    assert_eq!(outcome.dispatched_runs.len(), 1);
    let dispatched = &outcome.dispatched_runs[0];
    assert!(!dispatched.run_id.is_empty());
    assert!(!dispatched.session_id.is_empty());
    assert!(!dispatched.turn_id.is_empty());

    let row = sqlx::query(
        r#"SELECT r.state, r.session_id, r.turn_id, r.attempt, p.current_state, p.current_run_id,
                  t.metadata, t.input_summary, s.execution_profile_id, s.execution_profile_version
           FROM work_item_runs r
           JOIN work_item_runtime_projection p ON p.work_item_id = r.work_item_id
           JOIN turns t ON t.turn_id = r.turn_id
           JOIN sessions s ON s.session_id = r.session_id
           WHERE r.run_id = ?"#,
    )
    .bind(&dispatched.run_id)
    .fetch_one(&pool)
    .await
    .expect("run row");

    assert_eq!(row.get::<String, _>("state"), "running");
    assert_eq!(row.get::<String, _>("current_state"), "running");
    assert_eq!(row.get::<String, _>("current_run_id"), dispatched.run_id);
    assert_eq!(row.get::<i64, _>("attempt"), 1);
    assert_eq!(row.get::<String, _>("session_id"), dispatched.session_id);
    assert_eq!(row.get::<String, _>("turn_id"), dispatched.turn_id);
    assert_eq!(row.get::<String, _>("execution_profile_id"), "implementer");
    assert_eq!(row.get::<String, _>("execution_profile_version"), "1");

    let metadata: Value =
        serde_json::from_str(&row.get::<String, _>("metadata")).expect("metadata json");
    assert_eq!(metadata["dag_managed"], true);
    assert_eq!(metadata["task_id"], task_id);
    assert_eq!(metadata["run_id"], dispatched.run_id);
    assert_eq!(metadata["work_item_id"], dispatched.work_item_id);

    let input = row.get::<String, _>("input_summary");
    assert!(input.contains(&dispatched.work_item_id));
    assert!(input.contains("impl title"));
    assert!(input.contains("impl description"));

    cleanup_scheduler_tmux_sessions(&pool).await;
}

#[tokio::test]
async fn scheduler_uses_dag_task_planner_client_for_work_item_agents() {
    let _scope = GenericClientTestScope::new().await;
    let pool = test_pool().await;
    sqlx::query(
        "UPDATE execution_profiles SET supported_client_types = ? WHERE profile_id = 'implementer'",
    )
    .bind(json!(["pi", "claude_code", "generic"]).to_string())
    .execute(&pool)
    .await
    .expect("enable generic implementer profile");
    let task_id = insert_dag_task_with_planner_client(&pool, "generic").await;
    DagService::new(pool.clone())
        .apply_initial_dag(
            &task_id,
            &initial_plan(vec![draft("impl", "implementer", 0)], vec![]),
        )
        .await
        .expect("apply dag");

    let outcome = DagSchedulerService::new(pool.clone())
        .schedule_task(&task_id)
        .await
        .expect("schedule task");

    assert_eq!(outcome.dispatched_runs.len(), 1);
    let row = sqlx::query(
        r#"SELECT r.client_type AS run_client_type, s.client_type AS session_client_type
           FROM work_item_runs r
           JOIN sessions s ON s.session_id = r.session_id
           WHERE r.run_id = ?"#,
    )
    .bind(&outcome.dispatched_runs[0].run_id)
    .fetch_one(&pool)
    .await
    .expect("run client row");
    assert_eq!(row.get::<String, _>("run_client_type"), "generic");
    assert_eq!(row.get::<String, _>("session_client_type"), "generic");

    cleanup_scheduler_tmux_sessions(&pool).await;
}

#[tokio::test]
async fn scheduler_tick_is_idempotent_for_active_runs() {
    let pool = test_pool().await;
    let task_id = insert_task(&pool).await;
    DagService::new(pool.clone())
        .apply_initial_dag(
            &task_id,
            &initial_plan(vec![draft("impl", "implementer", 0)], vec![]),
        )
        .await
        .expect("apply dag");
    let scheduler = DagSchedulerService::new(pool.clone());

    let first = scheduler.schedule_task(&task_id).await.expect("first tick");
    let second = scheduler
        .schedule_task(&task_id)
        .await
        .expect("second tick");

    assert_eq!(first.dispatched_runs.len(), 1);
    assert_eq!(second.dispatched_runs.len(), 0);
    let run_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM work_item_runs")
        .fetch_one(&pool)
        .await
        .expect("run count");
    assert_eq!(run_count, 1);

    cleanup_scheduler_tmux_sessions(&pool).await;
}

#[tokio::test]
async fn scheduler_does_not_dispatch_blocked_downstream_work_items() {
    let pool = test_pool().await;
    let task_id = insert_task(&pool).await;
    DagService::new(pool.clone())
        .apply_initial_dag(
            &task_id,
            &initial_plan(
                vec![
                    draft("design", "planner", 0),
                    draft("impl", "implementer", 0),
                ],
                vec![edge("design", "impl")],
            ),
        )
        .await
        .expect("apply dag");

    let outcome = DagSchedulerService::new(pool.clone())
        .schedule_task(&task_id)
        .await
        .expect("schedule task");

    assert_eq!(outcome.dispatched_runs.len(), 1);
    let dispatched_title: String =
        sqlx::query_scalar("SELECT title FROM work_items WHERE work_item_id = ?")
            .bind(&outcome.dispatched_runs[0].work_item_id)
            .fetch_one(&pool)
            .await
            .expect("dispatched title");
    assert_eq!(dispatched_title, "design title");

    let impl_state: String = sqlx::query_scalar(
        r#"SELECT p.current_state
           FROM work_items wi JOIN work_item_runtime_projection p ON p.work_item_id = wi.work_item_id
           WHERE wi.title = 'impl title'"#,
    )
    .fetch_one(&pool)
    .await
    .expect("impl state");
    assert_eq!(impl_state, "blocked");

    cleanup_scheduler_tmux_sessions(&pool).await;
}
