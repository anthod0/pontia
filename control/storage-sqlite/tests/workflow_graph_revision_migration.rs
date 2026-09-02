use pontia_storage_sqlite::{connect_sqlite, run_migrations};

#[tokio::test]
async fn migration_backfills_the_immediately_preceding_workflow_schema_at_revision_one() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("workflow-revision-migration.db");
    let pool = connect_sqlite(&format!("sqlite://{}", db_path.display()))
        .await
        .expect("connect");

    sqlx::raw_sql(
        r#"
        CREATE TABLE workflows (
            workflow_id TEXT PRIMARY KEY NOT NULL,
            title TEXT NOT NULL,
            cwd TEXT NOT NULL,
            state TEXT NOT NULL,
            failure_message TEXT,
            created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
            updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
            started_at TEXT,
            completed_at TEXT,
            activating_node_id TEXT REFERENCES workflow_nodes(node_id)
        );
        CREATE TABLE workflow_nodes (
            node_id TEXT PRIMARY KEY NOT NULL,
            workflow_id TEXT NOT NULL REFERENCES workflows(workflow_id),
            parent_node_id TEXT REFERENCES workflow_nodes(node_id),
            title TEXT NOT NULL,
            instructions TEXT NOT NULL,
            inputs TEXT NOT NULL DEFAULT '[]',
            output TEXT NOT NULL,
            execution_profile_id TEXT,
            execution_profile_version TEXT,
            session_id TEXT,
            submitted_at TEXT,
            created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
            node_type TEXT NOT NULL DEFAULT 'agent',
            phase TEXT NOT NULL DEFAULT '',
            submitted_runtime_instance_id TEXT,
            exit_request_started_at TEXT
        );
        INSERT INTO workflows (workflow_id, title, cwd, state)
        VALUES ('wf_existing', 'Existing', '/work', 'running');
        INSERT INTO workflow_nodes
            (node_id, workflow_id, title, instructions, output, phase)
        VALUES ('node_existing', 'wf_existing', 'Existing node', 'Work', 'result.md', 'Build');
        "#,
    )
    .execute(&pool)
    .await
    .expect("create immediately preceding schema");

    sqlx::raw_sql(include_str!(
        "../migrations/0016_add_workflow_graph_revisions.sql"
    ))
    .execute(&pool)
    .await
    .expect("apply workflow graph revision migration");

    let workflow_revision: i64 = sqlx::query_scalar(
        "SELECT current_revision FROM workflows WHERE workflow_id = 'wf_existing'",
    )
    .fetch_one(&pool)
    .await
    .expect("workflow revision");
    let (introduced, retired): (i64, Option<i64>) = sqlx::query_as(
        "SELECT introduced_revision, retired_revision FROM workflow_nodes WHERE node_id = 'node_existing'",
    )
    .fetch_one(&pool)
    .await
    .expect("node membership");
    assert_eq!(workflow_revision, 1);
    assert_eq!(introduced, 1);
    assert_eq!(retired, None);
}

#[tokio::test]
async fn fresh_schema_enforces_immutable_node_history_and_revision_ranges() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("workflow-revision-constraints.db");
    let pool = connect_sqlite(&format!("sqlite://{}", db_path.display()))
        .await
        .expect("connect");
    run_migrations(&pool).await.expect("migrate fresh schema");

    sqlx::query(
        "INSERT INTO workflows (workflow_id, title, cwd, state) VALUES ('wf_a', 'A', '/a', 'running'), ('wf_b', 'B', '/b', 'running')",
    )
    .execute(&pool)
    .await
    .expect("insert workflows");
    sqlx::query(
        "INSERT INTO workflow_nodes (node_id, workflow_id, node_type, phase, title, instructions, output) VALUES ('node_a', 'wf_a', 'agent', 'Build', 'A', 'Do A', 'a.md')",
    )
    .execute(&pool)
    .await
    .expect("insert root");

    assert!(
        sqlx::query("UPDATE workflow_nodes SET title = 'Changed' WHERE node_id = 'node_a'")
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("DELETE FROM workflow_nodes WHERE node_id = 'node_a'")
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(sqlx::query(
        "INSERT INTO workflow_nodes (node_id, workflow_id, parent_node_id, node_type, phase, title, instructions, output) VALUES ('node_cross', 'wf_b', 'node_a', 'agent', 'Build', 'Cross', 'Cross', 'cross.md')",
    )
    .execute(&pool)
    .await
    .is_err());
    assert!(sqlx::query(
        "INSERT INTO workflow_nodes (node_id, workflow_id, node_type, phase, title, instructions, output, introduced_revision, retired_revision) VALUES ('node_bad_range', 'wf_a', 'agent', 'Build', 'Bad', 'Bad', 'bad.md', 2, 2)",
    )
    .execute(&pool)
    .await
    .is_err());
    assert!(sqlx::query(
        "INSERT INTO workflow_nodes (node_id, workflow_id, node_type, phase, title, instructions, output, introduced_revision) VALUES ('node_future', 'wf_a', 'agent', 'Build', 'Future', 'Future', 'future.md', 3)",
    )
    .execute(&pool)
    .await
    .is_err());
    assert!(
        sqlx::query("UPDATE workflows SET current_revision = 3 WHERE workflow_id = 'wf_a'")
            .execute(&pool)
            .await
            .is_err()
    );

    sqlx::query("UPDATE workflow_nodes SET retired_revision = 2 WHERE node_id = 'node_a'")
        .execute(&pool)
        .await
        .expect("retire once");
    assert!(
        sqlx::query("UPDATE workflow_nodes SET retired_revision = 3 WHERE node_id = 'node_a'")
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("UPDATE workflow_nodes SET retired_revision = NULL WHERE node_id = 'node_a'")
            .execute(&pool)
            .await
            .is_err()
    );
}
