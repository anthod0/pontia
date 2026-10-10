use pontia_storage_sqlite::{connect_sqlite, run_migrations};
use sqlx::Row;

#[tokio::test]
async fn rejects_tilde_prefixed_sqlite_database_urls() {
    let error = connect_sqlite("sqlite://~/.pontia/data/pontia.db")
        .await
        .expect_err("tilde path must be rejected");

    assert!(error.to_string().contains("tilde-prefixed SQLite paths"));
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
async fn migrations_preserve_removed_schema_contracts() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("control-plane.db");
    let database_url = format!("sqlite://{}", db_path.display());

    let pool = connect_sqlite(&database_url).await.expect("connect sqlite");
    run_migrations(&pool).await.expect("run migrations");

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
async fn session_overview_indexes_exist_and_back_their_queries() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("session-overview-indexes.db");
    let database_url = format!("sqlite://{}", db_path.display());
    let pool = connect_sqlite(&database_url).await.expect("connect sqlite");
    run_migrations(&pool).await.expect("run migrations");

    let indexes = sqlx::query("PRAGMA index_list(sessions)")
        .fetch_all(&pool)
        .await
        .expect("session indexes")
        .into_iter()
        .map(|row| row.get::<String, _>("name"))
        .collect::<Vec<_>>();
    for expected in [
        "idx_sessions_management_list",
        "idx_sessions_active_updated",
        "idx_sessions_unarchived_updated",
        "idx_sessions_workspace_unarchived_updated",
    ] {
        assert!(indexes.iter().any(|name| name == expected), "{expected}");
    }

    for (sql, expected_index) in [
        (
            "EXPLAIN QUERY PLAN SELECT session_id FROM sessions INDEXED BY idx_sessions_active_updated WHERE archived_at IS NULL AND state NOT IN ('exited', 'error') ORDER BY updated_at DESC, session_id DESC",
            "idx_sessions_active_updated",
        ),
        (
            "EXPLAIN QUERY PLAN SELECT session_id FROM sessions INDEXED BY idx_sessions_unarchived_updated WHERE archived_at IS NULL ORDER BY updated_at DESC, session_id DESC",
            "idx_sessions_unarchived_updated",
        ),
        (
            "EXPLAIN QUERY PLAN SELECT session_id FROM sessions INDEXED BY idx_sessions_workspace_unarchived_updated WHERE workspace_id = 'workspace-1' AND archived_at IS NULL ORDER BY updated_at DESC, session_id DESC",
            "idx_sessions_workspace_unarchived_updated",
        ),
    ] {
        let plan = sqlx::query(sql)
            .fetch_all(&pool)
            .await
            .expect("explain query")
            .into_iter()
            .map(|row| row.get::<String, _>("detail"))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(plan.contains(expected_index), "{plan}");
    }
}

#[tokio::test]
async fn session_runtimes_schema_uses_structured_runtime_fields_without_runtime_ref() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("runtime-bindings-schema.db");
    let database_url = format!("sqlite://{}", db_path.display());

    let pool = connect_sqlite(&database_url).await.expect("connect sqlite");
    run_migrations(&pool).await.expect("run migrations");

    let columns = sqlx::query("PRAGMA table_info(session_runtimes)")
        .fetch_all(&pool)
        .await
        .expect("session_runtimes columns")
        .into_iter()
        .map(|row| row.get::<String, _>("name"))
        .collect::<Vec<_>>();

    assert_eq!(
        columns,
        [
            "runtime_id",
            "session_id",
            "role",
            "state",
            "start_command",
            "tmux_socket_path",
            "tmux_pane_id",
            "process_fingerprint",
            "created_at"
        ]
    );
}
