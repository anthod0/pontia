use pontia_storage_sqlite::connect_sqlite;

#[tokio::test]
async fn removing_session_files_drops_shares_and_their_events() {
    let root = tempfile::tempdir().unwrap();
    let pool = connect_sqlite(&format!(
        "sqlite://{}",
        root.path().join("files.db").display()
    ))
    .await
    .unwrap();
    let migrations = sqlx::migrate!("./migrations");
    for migration in migrations.iter().filter(|migration| migration.version < 63) {
        sqlx::raw_sql(&migration.sql).execute(&pool).await.unwrap();
    }
    sqlx::query(
        "INSERT INTO sessions (session_id, client_type, state) VALUES ('s', 'generic', 'idle')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO session_files (file_id, session_id, share_id, path, name, size, media_type, created_at) VALUES ('file_1', 's', 'share_1', '/a.txt', 'a.txt', 1, 'application/octet-stream', '2026-01-01T00:00:00Z')")
        .execute(&pool).await.unwrap();
    for (id, event_type) in [
        ("shared", "session.files_shared"),
        ("kept", "session.title_updated"),
    ] {
        sqlx::query("INSERT INTO events (event_id, session_id, source, client_type, event_type, occurred_at, payload) VALUES (?, 's', 'external_api', 'generic', ?, '2026-01-01T00:00:00Z', '{}')")
            .bind(id).bind(event_type).execute(&pool).await.unwrap();
    }
    for (position, id, event_type) in [
        (1, "shared", "session.files_shared"),
        (2, "kept", "session.title_updated"),
    ] {
        sqlx::query("INSERT INTO workflow_agent_events (position, event_id, session_id, source, event_type, occurred_at, payload, created_at) VALUES (?, ?, 's', 'external_api', ?, '2026-01-01T00:00:00Z', '{}', '2026-01-01T00:00:00Z')")
            .bind(position).bind(id).bind(event_type).execute(&pool).await.unwrap();
    }
    let migration = migrations.iter().find(|m| m.version == 63).unwrap();
    sqlx::raw_sql(&migration.sql).execute(&pool).await.unwrap();

    let table: Option<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'session_files'",
    )
    .fetch_optional(&pool)
    .await
    .unwrap();
    assert_eq!(table, None);
    for query in [
        "SELECT event_id FROM events ORDER BY event_id",
        "SELECT event_id FROM domain_event_positions ORDER BY event_id",
        "SELECT event_id FROM workflow_agent_events ORDER BY event_id",
    ] {
        let ids: Vec<String> = sqlx::query_scalar(query).fetch_all(&pool).await.unwrap();
        assert_eq!(ids, ["kept"], "{query}");
    }
}
