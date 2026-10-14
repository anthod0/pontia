use pontia_storage_sqlite::connect_sqlite;

#[tokio::test]
async fn upgrade_preserves_history_and_non_trigger_schema() {
    let root = tempfile::tempdir().unwrap();
    let pool = connect_sqlite(&format!(
        "sqlite://{}",
        root.path().join("upgrade.db").display()
    ))
    .await
    .unwrap();
    for migration in sqlx::migrate!("./migrations")
        .iter()
        .filter(|migration| migration.version < 35)
    {
        sqlx::raw_sql(&migration.sql).execute(&pool).await.unwrap();
    }
    sqlx::raw_sql(
        r#"INSERT INTO sessions (session_id, client_type, state) VALUES ('session', 'generic', 'idle');
           INSERT INTO turns (turn_id, session_id, state, topology_status) VALUES ('turn', 'session', 'completed', 'root');
           INSERT INTO events (event_id, session_id, turn_id, source, client_type, event_type, occurred_at, turn_topology)
           VALUES ('event', 'session', 'turn', 'agent_adapter', 'generic', 'turn.started', '2026-01-01T00:00:00Z', '{"status":"root"}');
           INSERT INTO workflows (workflow_id, title, cwd, state) VALUES ('workflow', 'History', '/work', 'running');
           INSERT INTO workflow_nodes (node_id, workflow_id, title, instructions, output)
           VALUES ('retired', 'workflow', 'Old', 'Original', 'old.md');
           UPDATE workflow_nodes SET retired_revision = 2 WHERE node_id = 'retired';
           INSERT INTO workflow_nodes (node_id, workflow_id, title, instructions, output, introduced_revision)
           VALUES ('active', 'workflow', 'New', 'Replacement', 'new.md', 2);
           UPDATE workflows SET current_revision = 2 WHERE workflow_id = 'workflow';"#,
    )
    .execute(&pool)
    .await
    .unwrap();

    let schema_sql =
        "SELECT type, name, sql FROM sqlite_schema WHERE type <> 'trigger' ORDER BY type, name";
    let schema_before: Vec<(String, String, Option<String>)> =
        sqlx::query_as(schema_sql).fetch_all(&pool).await.unwrap();
    let history_sql = r#"SELECT 'event', json_array(event_id, session_id, turn_id, turn_topology) FROM events
        UNION ALL SELECT 'turn', json_array(turn_id, session_id, state, topology_status) FROM turns
        UNION ALL SELECT 'workflow', json_array(workflow_id, current_revision, state) FROM workflows
        UNION ALL SELECT 'node', json_array(node_id, workflow_id, parent_node_id, title, instructions, output, introduced_revision, retired_revision) FROM workflow_nodes
        ORDER BY 1, 2"#;
    let history_before: Vec<(String, String)> =
        sqlx::query_as(history_sql).fetch_all(&pool).await.unwrap();

    let mut tx = pool.begin().await.unwrap();
    sqlx::raw_sql(include_str!(
        "../migrations/0035_remove_defensive_triggers.sql"
    ))
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();

    let schema_after: Vec<(String, String, Option<String>)> =
        sqlx::query_as(schema_sql).fetch_all(&pool).await.unwrap();
    assert_eq!(schema_after, schema_before);
    let history_after: Vec<(String, String)> =
        sqlx::query_as(history_sql).fetch_all(&pool).await.unwrap();
    assert_eq!(history_after, history_before);
    assert!(
        sqlx::query("PRAGMA foreign_key_check")
            .fetch_all(&pool)
            .await
            .unwrap()
            .is_empty()
    );
}
