use pontia::storage::sqlite::{
    connect_sqlite, repositories::tasks::SqliteTaskRepository, run_migrations,
};
use serde_json::json;

async fn test_pool() -> sqlx::SqlitePool {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("sqlite_task_repository.db");
    let _kept_dir = dir.keep();
    let database_url = format!("sqlite://{}", db_path.display());
    let pool = connect_sqlite(&database_url).await.expect("connect");
    run_migrations(&pool).await.expect("migrate");
    pool
}

#[tokio::test]
async fn sqlite_task_repository_lists_tasks_with_existing_order_and_json_fields_as_strings() {
    let pool = test_pool().await;
    sqlx::query(
        r#"INSERT INTO tasks
           (task_id, state, input, metadata, created_at, updated_at)
           VALUES
           ('task_b', 'created', 'second', ?, '2026-06-15T12:00:00Z', '2026-06-15T12:00:00Z'),
           ('task_a', 'running', 'first', ?, '2026-06-15T12:00:00Z', '2026-06-15T12:00:01Z'),
           ('task_c', 'created', 'newest', ?, '2026-06-15T12:00:01Z', '2026-06-15T12:00:01Z')"#,
    )
    .bind(json!({"rank": 2}).to_string())
    .bind(json!({"rank": 1}).to_string())
    .bind(json!({"rank": 3}).to_string())
    .execute(&pool)
    .await
    .expect("insert tasks");

    let repository = SqliteTaskRepository::new(pool);
    let rows = repository.list_tasks().await.expect("list tasks");

    let ids: Vec<_> = rows.iter().map(|row| row.task_id.as_str()).collect();
    assert_eq!(ids, vec!["task_c", "task_a", "task_b"]);
    assert_eq!(rows[0].metadata, json!({"rank": 3}).to_string());
}

#[tokio::test]
async fn sqlite_task_repository_lists_task_events_with_existing_order_and_payload_as_string() {
    let pool = test_pool().await;
    sqlx::query(
        "INSERT INTO tasks (task_id, state, input) VALUES ('task_events', 'created', 'input')",
    )
    .execute(&pool)
    .await
    .expect("insert task");
    sqlx::query(
        r#"INSERT INTO task_events (event_id, task_id, event_type, payload, created_at)
           VALUES
           ('evt_b', 'task_events', 'task.updated', ?, '2026-06-15T12:00:00Z'),
           ('evt_a', 'task_events', 'task.created', ?, '2026-06-15T12:00:00Z'),
           ('evt_c', 'task_events', 'task.done', ?, '2026-06-15T12:00:01Z')"#,
    )
    .bind(json!({"order": "b"}).to_string())
    .bind(json!({"order": "a"}).to_string())
    .bind(json!({"order": "c"}).to_string())
    .execute(&pool)
    .await
    .expect("insert task events");

    let repository = SqliteTaskRepository::new(pool);
    let rows = repository
        .list_task_events("task_events")
        .await
        .expect("list task events");

    let ids: Vec<_> = rows.iter().map(|row| row.event_id.as_str()).collect();
    assert_eq!(ids, vec!["evt_a", "evt_b", "evt_c"]);
    assert_eq!(rows[0].payload, json!({"order": "a"}).to_string());
}
