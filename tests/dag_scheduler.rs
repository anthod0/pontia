#![cfg(any())]

#[path = "support/generic_client.rs"]
mod generic_client;

use generic_client::GenericClientTestScope;
use pontia::{
    application::{
        DagPatch, DagSchedulerService, DagService, PatchOperation, SqliteDagGraphStore,
        SubmitPlanPayload, WorkItemDraft, WorkItemEdgeDraft,
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
    let targets: Vec<String> = sqlx::query_scalar(
        "SELECT json_extract(metadata, '$.tmux.session_name') FROM runtime_bindings WHERE json_extract(metadata, '$.tmux.session_name') IS NOT NULL",
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    for target in targets {
        let _ = Command::new("tmux")
            .args(["kill-session", "-t", &target])
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
async fn scheduler_does_not_dispatch_while_blocking_signal_is_open() {
    let pool = test_pool().await;
    let task_id = insert_task(&pool).await;
    DagService::new(pool.clone())
        .apply_initial_dag(
            &task_id,
            &initial_plan(vec![draft("impl", "implementer", 0)], vec![]),
        )
        .await
        .expect("apply dag");
    sqlx::query(
        r#"INSERT INTO dag_signals (signal_id, task_id, kind, summary, severity, state)
           VALUES ('sig_blocking_open', ?, 'needs_input', 'Need input', 'medium', 'open')"#,
    )
    .bind(&task_id)
    .execute(&pool)
    .await
    .expect("insert signal");

    let outcome = DagSchedulerService::new(pool.clone())
        .schedule_task(&task_id)
        .await
        .expect("schedule task");

    assert_eq!(outcome.dispatched_runs.len(), 0);
    let run_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM work_item_runs")
        .fetch_one(&pool)
        .await
        .expect("run count");
    assert_eq!(run_count, 0);

    cleanup_scheduler_tmux_sessions(&pool).await;
}

#[tokio::test]
async fn scheduler_does_not_dispatch_while_task_is_replanning() {
    let pool = test_pool().await;
    let task_id = insert_task(&pool).await;
    DagService::new(pool.clone())
        .apply_initial_dag(
            &task_id,
            &initial_plan(vec![draft("impl", "implementer", 0)], vec![]),
        )
        .await
        .expect("apply dag");
    sqlx::query("UPDATE tasks SET state = 'replanning' WHERE task_id = ?")
        .bind(&task_id)
        .execute(&pool)
        .await
        .expect("mark replanning");

    let outcome = DagSchedulerService::new(pool.clone())
        .schedule_task(&task_id)
        .await
        .expect("schedule task");

    assert_eq!(outcome.dispatched_runs.len(), 0);

    cleanup_scheduler_tmux_sessions(&pool).await;
}

#[tokio::test]
async fn scheduler_allows_replan_anchor_to_start_new_path() {
    let pool = test_pool().await;
    let task_id = insert_task(&pool).await;
    let dag = DagService::new(pool.clone());
    dag.apply_initial_dag(
        &task_id,
        &initial_plan(
            vec![
                draft("current", "implementer", 0),
                draft("old_next", "implementer", 0),
            ],
            vec![edge("current", "old_next")],
        ),
    )
    .await
    .expect("apply dag");
    let graph_store = SqliteDagGraphStore::new(pool.clone());
    let graph = graph_store.task_graph(&task_id).await.expect("task graph");
    let current_id = graph
        .work_items
        .iter()
        .find(|work_item| work_item.title == "current title")
        .expect("current id")
        .work_item_id
        .clone();
    let old_next_id = graph
        .work_items
        .iter()
        .find(|work_item| work_item.title == "old_next title")
        .expect("old next id")
        .work_item_id
        .clone();
    sqlx::query(
        "UPDATE work_item_runtime_projection SET current_state = 'replan_anchor' WHERE work_item_id = ?",
    )
    .bind(&current_id)
    .execute(&pool)
    .await
    .expect("mark anchor");

    dag.apply_patch(
        &task_id,
        &DagPatch {
            base_revision: None,
            summary: "replace old path".to_string(),
            anchor_work_item_id: None,
            supersede_policy: "explicit_only".to_string(),
            operations: vec![
                PatchOperation::SupersedeWorkItem {
                    work_item_id: old_next_id,
                    reason: "replanned".to_string(),
                },
                PatchOperation::AddWorkItem {
                    work_item: draft("new_next", "implementer", 0),
                },
                PatchOperation::AddEdge {
                    edge: edge(&current_id, "new_next"),
                },
            ],
        },
    )
    .await
    .expect("apply patch");

    let outcome = DagSchedulerService::new(pool.clone())
        .schedule_task(&task_id)
        .await
        .expect("schedule task");

    assert_eq!(outcome.dispatched_runs.len(), 1);
    let dispatched_title = graph_store
        .get_work_item(&outcome.dispatched_runs[0].work_item_id)
        .await
        .expect("dispatched work item")
        .expect("dispatched work item")
        .title;
    assert_eq!(dispatched_title, "new_next title");

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
    let graph_store = SqliteDagGraphStore::new(pool.clone());
    let dispatched_title = graph_store
        .get_work_item(&outcome.dispatched_runs[0].work_item_id)
        .await
        .expect("dispatched work item")
        .expect("dispatched work item")
        .title;
    assert_eq!(dispatched_title, "design title");

    let impl_id = graph_store
        .task_graph(&task_id)
        .await
        .expect("task graph")
        .work_items
        .into_iter()
        .find(|work_item| work_item.title == "impl title")
        .expect("impl work item")
        .work_item_id;
    let impl_state: String = sqlx::query_scalar(
        "SELECT current_state FROM work_item_runtime_projection WHERE work_item_id = ?",
    )
    .bind(impl_id)
    .fetch_one(&pool)
    .await
    .expect("impl state");
    assert_eq!(impl_state, "blocked");

    cleanup_scheduler_tmux_sessions(&pool).await;
}

#[tokio::test]
async fn scheduler_dispatches_ready_work_items_by_priority() {
    let pool = test_pool().await;
    let task_id = insert_task(&pool).await;
    DagService::new(pool.clone())
        .apply_initial_dag(
            &task_id,
            &initial_plan(
                vec![
                    draft("low", "implementer", 1),
                    draft("high", "implementer", 50),
                ],
                vec![],
            ),
        )
        .await
        .expect("apply dag");

    let outcome = DagSchedulerService::new(pool.clone())
        .schedule_task(&task_id)
        .await
        .expect("schedule task");

    assert_eq!(outcome.dispatched_runs.len(), 2);
    let graph_store = SqliteDagGraphStore::new(pool.clone());
    let first_title = graph_store
        .get_work_item(&outcome.dispatched_runs[0].work_item_id)
        .await
        .expect("first work item")
        .expect("first work item")
        .title;
    let second_title = graph_store
        .get_work_item(&outcome.dispatched_runs[1].work_item_id)
        .await
        .expect("second work item")
        .expect("second work item")
        .title;
    assert_eq!(first_title, "high title");
    assert_eq!(second_title, "low title");

    cleanup_scheduler_tmux_sessions(&pool).await;
}
