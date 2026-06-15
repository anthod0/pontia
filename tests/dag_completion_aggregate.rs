use pontia::{
    application::{
        AgentToolContext, AgentToolMode, DagRunResultService, GraphRuntimeConfig,
        SubmitResultPayload,
    },
    storage::sqlite::{connect_sqlite, run_migrations},
};
use sqlx::{Row, SqlitePool};

async fn test_pool() -> SqlitePool {
    let db = connect_sqlite("sqlite://:memory:").await.expect("connect");
    run_migrations(&db).await.expect("migrate");
    db
}

fn graph_config(tempdir: &tempfile::TempDir) -> GraphRuntimeConfig {
    GraphRuntimeConfig {
        enabled: true,
        db_dir: Some(tempdir.path().join("graph.lbug").display().to_string()),
    }
}

#[tokio::test]
async fn submit_result_marks_task_completed_from_runtime_projection_even_without_graph_work_items()
{
    let pool = test_pool().await;
    let graph_dir = tempfile::tempdir().expect("temp graph dir");
    let graph = graph_config(&graph_dir);
    let task_id = "task_aggregate_done";
    let session_id = "sess_aggregate_done";
    let turn_id = "turn_aggregate_done";

    sqlx::query("INSERT INTO tasks (task_id, state, input) VALUES (?, 'running', 'test task')")
        .bind(task_id)
        .execute(&pool)
        .await
        .expect("insert task");

    let work_item_id = "wi_aggregate_done".to_string();
    let run_id = "run_aggregate_done";
    sqlx::query(
        r#"INSERT INTO work_item_runtime_projection (
                work_item_id, task_id, current_state, current_attempt, priority, optional, parallelizable
           ) VALUES (?, ?, 'running', 1, 0, 0, 1)"#,
    )
    .bind(&work_item_id)
    .bind(task_id)
    .execute(&pool)
    .await
    .expect("insert runtime projection");

    sqlx::query(
        r#"INSERT INTO sessions (session_id, client_type, state, current_turn_id, metadata)
           VALUES (?, 'generic', 'busy', ?, '{}')"#,
    )
    .bind(session_id)
    .bind(turn_id)
    .execute(&pool)
    .await
    .expect("insert session");
    sqlx::query(
        "INSERT INTO turns (turn_id, session_id, state, metadata) VALUES (?, ?, 'running', '{}')",
    )
    .bind(turn_id)
    .bind(session_id)
    .execute(&pool)
    .await
    .expect("insert turn");
    sqlx::query(
        "INSERT INTO runtime_bindings (session_id, runtime_kind, metadata) VALUES (?, 'generic', '{}')",
    )
    .bind(session_id)
    .execute(&pool)
    .await
    .expect("insert runtime binding");
    sqlx::query(
        r#"INSERT INTO work_item_runs (
                run_id, work_item_id, task_id, attempt, state, session_id, turn_id,
                client_type, execution_profile_id, rendered_prompt_ref
           ) VALUES (?, ?, ?, 1, 'running', ?, ?, 'generic', 'implementer', 'inline')"#,
    )
    .bind(run_id)
    .bind(&work_item_id)
    .bind(task_id)
    .bind(session_id)
    .bind(turn_id)
    .execute(&pool)
    .await
    .expect("insert run");
    sqlx::query(
        r#"UPDATE work_item_runtime_projection
           SET current_state = 'running', current_run_id = ?, current_attempt = 1,
               session_id = ?, turn_id = ?
           WHERE task_id = ? AND work_item_id = ?"#,
    )
    .bind(run_id)
    .bind(session_id)
    .bind(turn_id)
    .bind(task_id)
    .bind(&work_item_id)
    .execute(&pool)
    .await
    .expect("update runtime");

    DagRunResultService::with_graph(pool.clone(), graph)
        .submit_tool_result(
            &AgentToolContext {
                session_id: session_id.to_string(),
                turn_id: turn_id.to_string(),
                client_type: "generic".to_string(),
                runtime_instance_id: "rt_aggregate_done".to_string(),
                task_id: task_id.to_string(),
                mode: AgentToolMode::Execution {
                    run_id: run_id.to_string(),
                    work_item_id: work_item_id.clone(),
                },
            },
            SubmitResultPayload {
                status: "completed".to_string(),
                summary: "all done".to_string(),
                outputs: vec![],
                failure: None,
                signals: vec![],
            },
        )
        .await
        .expect("submit result");

    let row = sqlx::query("SELECT state FROM tasks WHERE task_id = ?")
        .bind(task_id)
        .fetch_one(&pool)
        .await
        .expect("task row");
    assert_eq!(row.get::<String, _>("state"), "completed");

    let completed_events: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM task_events WHERE task_id = ? AND event_type = 'task.completed'",
    )
    .bind(task_id)
    .fetch_one(&pool)
    .await
    .expect("task.completed count");
    assert_eq!(completed_events, 1);
}
