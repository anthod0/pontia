use pontia_storage_sqlite::{connect_sqlite, run_migrations};

#[tokio::test]
async fn migration_upgrades_the_immediately_preceding_schema_without_an_active_patch() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("workflow-patch-migration.db");
    let pool = connect_sqlite(&format!("sqlite://{}", db_path.display()))
        .await
        .expect("connect");
    sqlx::raw_sql(
        r#"
        CREATE TABLE sessions (session_id TEXT PRIMARY KEY NOT NULL);
        CREATE TABLE turns (
            turn_id TEXT PRIMARY KEY NOT NULL,
            session_id TEXT NOT NULL REFERENCES sessions(session_id)
        );
        CREATE TABLE workflows (
            workflow_id TEXT PRIMARY KEY NOT NULL,
            title TEXT NOT NULL,
            cwd TEXT NOT NULL,
            state TEXT NOT NULL,
            current_revision INTEGER NOT NULL DEFAULT 1,
            activating_node_id TEXT,
            created_at TEXT,
            updated_at TEXT
        );
        CREATE TABLE workflow_nodes (
            node_id TEXT PRIMARY KEY NOT NULL,
            workflow_id TEXT NOT NULL REFERENCES workflows(workflow_id)
        );
        INSERT INTO workflows
            (workflow_id, title, cwd, state, current_revision)
        VALUES ('wf_existing', 'Existing', '/work', 'running', 1);
        "#,
    )
    .execute(&pool)
    .await
    .expect("create preceding schema");

    sqlx::raw_sql(include_str!(
        "../migrations/0017_add_workflow_patch_requests.sql"
    ))
    .execute(&pool)
    .await
    .expect("apply Workflow Patch migration");

    let pointers: (Option<String>, Option<String>) = sqlx::query_as(
        "SELECT active_patch_id, active_replanner_session_id FROM workflows WHERE workflow_id = 'wf_existing'",
    )
    .fetch_one(&pool)
    .await
    .expect("load active pointers");
    assert_eq!(pointers, (None, None));
}

#[tokio::test]
async fn replanner_blocking_migration_upgrades_the_preceding_patch_schema() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("workflow-replanner-blocking-migration.db");
    let pool = connect_sqlite(&format!("sqlite://{}", db_path.display()))
        .await
        .expect("connect");
    sqlx::raw_sql("CREATE TABLE workflow_patches (patch_id TEXT PRIMARY KEY NOT NULL)")
        .execute(&pool)
        .await
        .expect("create preceding Patch schema");

    sqlx::raw_sql(include_str!(
        "../migrations/0018_complete_workflow_replanner_blocking.sql"
    ))
    .execute(&pool)
    .await
    .expect("apply Re-planner blocking migration");

    let columns: Vec<String> =
        sqlx::query_scalar("SELECT name FROM pragma_table_info('workflow_patches') ORDER BY cid")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(columns, ["patch_id", "replanner_exit_requested_at"]);
}

#[tokio::test]
async fn replanner_creation_token_migration_rejects_duplicate_sessions() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("workflow-replanner-token-migration.db");
    let pool = connect_sqlite(&format!("sqlite://{}", db_path.display()))
        .await
        .expect("connect");
    sqlx::raw_sql(
        r#"CREATE TABLE sessions (
            session_id TEXT PRIMARY KEY NOT NULL,
            metadata TEXT NOT NULL DEFAULT '{}'
        )"#,
    )
    .execute(&pool)
    .await
    .expect("create preceding Session schema");
    sqlx::raw_sql(include_str!(
        "../migrations/0019_fence_workflow_replanner_creation.sql"
    ))
    .execute(&pool)
    .await
    .expect("apply creation-token fence");

    sqlx::query("INSERT INTO sessions (session_id, metadata) VALUES (?, ?)")
        .bind("sess_one")
        .bind(r#"{"workflow_replanner_creation_token":"token_one"}"#)
        .execute(&pool)
        .await
        .expect("first Session");
    let duplicate = sqlx::query("INSERT INTO sessions (session_id, metadata) VALUES (?, ?)")
        .bind("sess_two")
        .bind(r#"{"workflow_replanner_creation_token":"token_one"}"#)
        .execute(&pool)
        .await;
    assert!(duplicate.is_err());
}

#[tokio::test]
async fn fresh_schema_enforces_one_active_patch_per_workflow() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("workflow-patch-constraints.db");
    let pool = connect_sqlite(&format!("sqlite://{}", db_path.display()))
        .await
        .expect("connect");
    run_migrations(&pool).await.expect("migrate fresh schema");
    sqlx::raw_sql(
        r#"
        INSERT INTO sessions (session_id, client_type, state)
        VALUES ('sess_requester', 'pi', 'working');
        INSERT INTO turns (turn_id, session_id, state, topology_status)
        VALUES ('turn_requester', 'sess_requester', 'running', 'root');
        INSERT INTO workflows (workflow_id, title, cwd, state)
        VALUES ('wf_patch', 'Patch', '/work', 'running');
        INSERT INTO workflow_nodes
            (node_id, workflow_id, node_type, phase, title, instructions, output)
        VALUES ('node_requester', 'wf_patch', 'agent', 'Build', 'Requester', 'Work', 'result.md');
        "#,
    )
    .execute(&pool)
    .await
    .expect("seed request facts");

    for patch_id in ["patch_one", "patch_two"] {
        let result = sqlx::query(
            r#"INSERT INTO workflow_patches
               (patch_id, workflow_id, requesting_node_id, requesting_session_id,
                requesting_turn_id, requesting_runtime_instance_id, replanner_creation_token,
                base_revision, state, request_document_ref, request_size_bytes)
               VALUES (?, 'wf_patch', 'node_requester', 'sess_requester', 'turn_requester',
                       'runtime_requester', ?, 1, 'requested', 'patches/request.md', 1)"#,
        )
        .bind(patch_id)
        .bind(format!("token_{patch_id}"))
        .execute(&pool)
        .await;
        if patch_id == "patch_one" {
            result.expect("first active Patch");
        } else {
            assert!(result.is_err(), "second active Patch must be rejected");
        }
    }
}
