use pontia_storage_sqlite::{connect_sqlite, normalize_sqlite_database_url, run_migrations};
use sqlx::Row;

#[test]
fn expands_tilde_sqlite_database_urls_before_connecting() {
    let normalized =
        normalize_sqlite_database_url("sqlite://~/.pontia/data/pontia.db", "/home/alice")
            .expect("normalize");

    assert_eq!(normalized, "sqlite:///home/alice/.pontia/data/pontia.db");
}

#[tokio::test]
async fn sqlite_connections_use_wal_journal_and_ten_second_busy_timeout() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("connection-options.db");
    let database_url = format!("sqlite://{}", db_path.display());

    let pool = connect_sqlite(&database_url).await.expect("connect sqlite");

    let journal_mode: String = sqlx::query_scalar("PRAGMA journal_mode")
        .fetch_one(&pool)
        .await
        .expect("query journal_mode");
    let busy_timeout: i64 = sqlx::query_scalar("PRAGMA busy_timeout")
        .fetch_one(&pool)
        .await
        .expect("query busy_timeout");

    assert_eq!(journal_mode, "wal");
    assert_eq!(busy_timeout, 10_000);
}

#[tokio::test]
async fn connects_to_sqlite_and_runs_migrations() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("control-plane.db");
    let database_url = format!("sqlite://{}", db_path.display());

    let pool = connect_sqlite(&database_url).await.expect("connect sqlite");
    run_migrations(&pool).await.expect("run migrations");

    let migration_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations")
        .fetch_one(&pool)
        .await
        .expect("query migrations");

    assert_eq!(migration_count, 6);

    let event_columns = sqlx::query("PRAGMA table_info(events)")
        .fetch_all(&pool)
        .await
        .expect("events columns")
        .into_iter()
        .map(|row| row.get::<String, _>("name"))
        .collect::<Vec<_>>();
    assert!(!event_columns.contains(&"seq".to_string()));

    let idempotency_table_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'idempotency_keys'",
    )
    .fetch_one(&pool)
    .await
    .expect("inspect idempotency_keys table");
    assert_eq!(idempotency_table_count, 0);

    let ingest_warnings_table_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'ingest_warnings'",
    )
    .fetch_one(&pool)
    .await
    .expect("inspect ingest_warnings table");
    assert_eq!(ingest_warnings_table_count, 0);
}

#[tokio::test]
async fn runtime_bindings_schema_uses_structured_runtime_fields_without_runtime_ref() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("runtime-bindings-schema.db");
    let database_url = format!("sqlite://{}", db_path.display());

    let pool = connect_sqlite(&database_url).await.expect("connect sqlite");
    run_migrations(&pool).await.expect("run migrations");

    let columns = sqlx::query("PRAGMA table_info(runtime_bindings)")
        .fetch_all(&pool)
        .await
        .expect("runtime_bindings columns")
        .into_iter()
        .map(|row| row.get::<String, _>("name"))
        .collect::<Vec<_>>();

    assert!(columns.contains(&"runtime_instance_id".to_string()));
    assert!(columns.contains(&"start_command".to_string()));
    assert!(columns.contains(&"launch_cwd".to_string()));
    assert!(columns.contains(&"last_seen_at".to_string()));
    assert!(columns.contains(&"tmux_socket_path".to_string()));
    assert!(columns.contains(&"tmux_pane_id".to_string()));
    assert!(!columns.contains(&"runtime_ref".to_string()));
}
