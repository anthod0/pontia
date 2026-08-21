use pontia_storage_sqlite::{
    connect_sqlite,
    repositories::workflows::{
        CreateWorkflowNodeRecord, CreateWorkflowRecord, SqliteWorkflowRepository,
    },
    run_migrations,
};
use serde_json::json;

async fn test_pool() -> (sqlx::SqlitePool, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("sqlite_workflow_repository.db");
    let database_url = format!("sqlite://{}", db_path.display());
    let pool = connect_sqlite(&database_url).await.expect("connect");
    run_migrations(&pool).await.expect("migrate");
    (pool, dir)
}

#[tokio::test]
async fn repository_persists_workflow_nodes_bindings_submissions_and_ordered_events() {
    let (pool, _pontia_home) = test_pool().await;
    let repository = SqliteWorkflowRepository::new(pool.clone());
    sqlx::query(
        "INSERT INTO sessions (session_id, client_type, state) VALUES ('session_1', 'pi', 'starting')",
    )
    .execute(&pool)
    .await
    .expect("create associated session");
    repository
        .create_workflow(CreateWorkflowRecord {
            workflow_id: "wf_1".to_string(),
            title: "Draft release notes".to_string(),
            cwd: "/work/project".to_string(),
            state: "pending".to_string(),
        })
        .await
        .expect("create workflow");
    repository
        .create_node(CreateWorkflowNodeRecord {
            node_id: "node_1".to_string(),
            workflow_id: "wf_1".to_string(),
            parent_node_id: None,
            phase: "Drafting".to_string(),
            title: "Writer".to_string(),
            instructions: "Write the release notes.".to_string(),
            inputs: json!(["brief.md", "changes.md"]).to_string(),
            output: "release.md".to_string(),
            execution_profile_id: Some("writer".to_string()),
            execution_profile_version: Some("3".to_string()),
        })
        .await
        .expect("create root node");
    repository
        .create_node(CreateWorkflowNodeRecord {
            node_id: "node_2".to_string(),
            workflow_id: "wf_1".to_string(),
            parent_node_id: Some("node_1".to_string()),
            phase: "Review".to_string(),
            title: "Reviewer".to_string(),
            instructions: "Review the release notes.".to_string(),
            inputs: json!(["release.md"]).to_string(),
            output: "approved.md".to_string(),
            execution_profile_id: None,
            execution_profile_version: None,
        })
        .await
        .expect("create child node");
    repository
        .start_workflow("wf_1", "evt_1")
        .await
        .expect("start workflow");

    repository
        .bind_node_session("node_1", "session_1")
        .await
        .expect("bind session");
    repository
        .record_node_submission("node_1", "runtime_session_1")
        .await
        .expect("record submission");
    repository
        .append_event("evt_2", "wf_1", "workflow.note", r#"{"order":2}"#)
        .await
        .expect("append second event");
    repository
        .append_event("evt_3", "wf_1", "workflow.note", r#"{"order":3}"#)
        .await
        .expect("append third event");

    let workflow = repository
        .get_workflow("wf_1")
        .await
        .expect("load workflow")
        .expect("workflow exists");
    assert_eq!(workflow.title, "Draft release notes");
    assert_eq!(workflow.cwd, "/work/project");
    assert_eq!(workflow.state, "running");

    let nodes = repository.list_nodes("wf_1").await.expect("list nodes");
    assert_eq!(nodes.len(), 2);
    assert_eq!(nodes[0].node_id, "node_1");
    assert_eq!(nodes[0].phase, "Drafting");
    assert_eq!(nodes[0].inputs, r#"["brief.md","changes.md"]"#);
    assert_eq!(nodes[0].session_id.as_deref(), Some("session_1"));
    assert!(nodes[0].submitted_at.is_some());
    assert_eq!(
        nodes[0].submitted_runtime_instance_id.as_deref(),
        Some("runtime_session_1")
    );
    assert_eq!(nodes[1].parent_node_id.as_deref(), Some("node_1"));
    assert_eq!(nodes[1].phase, "Review");
    assert_eq!(nodes[1].output, "approved.md");
    assert_eq!(
        repository
            .get_node("node_1")
            .await
            .expect("get node")
            .expect("node exists")
            .phase,
        "Drafting"
    );
    assert_eq!(
        repository
            .get_node_by_session("session_1")
            .await
            .expect("get node by session")
            .expect("bound node exists")
            .phase,
        "Drafting"
    );

    let events = repository.list_events("wf_1").await.expect("list events");
    assert_eq!(events.len(), 3);
    assert_eq!(events[0].sequence, 1);
    assert_eq!(events[0].event_id, "evt_1");
    assert_eq!(events[0].event_type, "workflow.started");
    assert_eq!(events[1].sequence, 2);
    assert_eq!(events[1].event_id, "evt_2");
    assert_eq!(events[2].sequence, 3);
    assert_eq!(events[2].event_id, "evt_3");
}

#[tokio::test]
async fn starting_a_pending_workflow_atomically_updates_state_and_appends_started_event() {
    let (pool, _pontia_home) = test_pool().await;
    let repository = SqliteWorkflowRepository::new(pool);
    repository
        .create_workflow(CreateWorkflowRecord {
            workflow_id: "wf_start".to_string(),
            title: "Start me".to_string(),
            cwd: "/work/project".to_string(),
            state: "pending".to_string(),
        })
        .await
        .expect("create workflow");

    repository
        .start_workflow("wf_start", "evt_started")
        .await
        .expect("start workflow");

    let workflow = repository
        .get_workflow("wf_start")
        .await
        .expect("load workflow")
        .expect("workflow exists");
    assert_eq!(workflow.state, "running");
    assert!(workflow.started_at.is_some());
    let events = repository
        .list_events("wf_start")
        .await
        .expect("list events");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].sequence, 1);
    assert_eq!(events[0].event_type, "workflow.started");
    assert_eq!(events[0].payload, "{}");

    let error = repository
        .start_workflow("wf_start", "evt_duplicate")
        .await
        .expect_err("running workflow cannot start twice");
    assert!(error.to_string().contains("pending"));
    assert_eq!(
        repository
            .list_events("wf_start")
            .await
            .expect("list events after rejected start")
            .len(),
        1
    );

    repository
        .create_workflow(CreateWorkflowRecord {
            workflow_id: "wf_rollback".to_string(),
            title: "Rollback me".to_string(),
            cwd: "/work/project".to_string(),
            state: "pending".to_string(),
        })
        .await
        .expect("create rollback workflow");
    let error = repository
        .start_workflow("wf_rollback", "evt_started")
        .await
        .expect_err("duplicate event id rejects transition");
    assert!(error.to_string().contains("UNIQUE"));
    let workflow = repository
        .get_workflow("wf_rollback")
        .await
        .expect("load rollback workflow")
        .expect("rollback workflow exists");
    assert_eq!(workflow.state, "pending");
    assert_eq!(workflow.started_at, None);
}

#[tokio::test]
async fn completing_a_running_workflow_atomically_updates_state_and_appends_completed_event() {
    let (pool, _pontia_home) = test_pool().await;
    let repository = SqliteWorkflowRepository::new(pool);
    repository
        .create_workflow(CreateWorkflowRecord {
            workflow_id: "wf_complete".to_string(),
            title: "Complete me".to_string(),
            cwd: "/work/project".to_string(),
            state: "pending".to_string(),
        })
        .await
        .expect("create workflow");
    repository
        .start_workflow("wf_complete", "evt_started")
        .await
        .expect("start workflow");

    repository
        .complete_workflow("wf_complete", "evt_completed")
        .await
        .expect("complete workflow");

    let workflow = repository
        .get_workflow("wf_complete")
        .await
        .expect("load workflow")
        .expect("workflow exists");
    assert_eq!(workflow.state, "completed");
    assert!(workflow.completed_at.is_some());
    let events = repository
        .list_events("wf_complete")
        .await
        .expect("list events");
    assert_eq!(events.len(), 2);
    assert_eq!(events[1].sequence, 2);
    assert_eq!(events[1].event_type, "workflow.completed");

    repository
        .create_workflow(CreateWorkflowRecord {
            workflow_id: "wf_complete_rollback".to_string(),
            title: "Rollback completion".to_string(),
            cwd: "/work/project".to_string(),
            state: "pending".to_string(),
        })
        .await
        .expect("create rollback workflow");
    repository
        .start_workflow("wf_complete_rollback", "evt_rollback_started")
        .await
        .expect("start rollback workflow");
    let error = repository
        .complete_workflow("wf_complete_rollback", "evt_completed")
        .await
        .expect_err("duplicate event id must roll back completion");
    assert!(error.to_string().contains("UNIQUE"));
    let workflow = repository
        .get_workflow("wf_complete_rollback")
        .await
        .expect("load rollback workflow")
        .expect("workflow exists");
    assert_eq!(workflow.state, "running");
    assert_eq!(workflow.completed_at, None);
}

#[tokio::test]
async fn node_activation_claim_blocks_pause_until_the_session_is_bound() {
    let (pool, _pontia_home) = test_pool().await;
    let repository = SqliteWorkflowRepository::new(pool);
    repository
        .create_workflow(CreateWorkflowRecord {
            workflow_id: "wf_activation_gate".to_string(),
            title: "Activation gate".to_string(),
            cwd: "/work/project".to_string(),
            state: "running".to_string(),
        })
        .await
        .expect("create workflow");
    repository
        .create_node(CreateWorkflowNodeRecord {
            node_id: "node_activation_gate".to_string(),
            workflow_id: "wf_activation_gate".to_string(),
            parent_node_id: None,
            phase: "Build".to_string(),
            title: "Builder".to_string(),
            instructions: "Build it".to_string(),
            inputs: "[]".to_string(),
            output: "result.md".to_string(),
            execution_profile_id: None,
            execution_profile_version: None,
        })
        .await
        .expect("create node");

    repository
        .claim_node_activation("wf_activation_gate", "node_activation_gate")
        .await
        .expect("claim activation");
    assert!(
        repository
            .pause_workflow("wf_activation_gate", "evt_pause_blocked")
            .await
            .is_err()
    );

    repository
        .finish_node_activation("node_activation_gate", "session_activation_gate")
        .await
        .expect("finish activation");
    repository
        .pause_workflow("wf_activation_gate", "evt_paused")
        .await
        .expect("pause after activation");
    assert_eq!(
        repository
            .get_workflow("wf_activation_gate")
            .await
            .expect("load workflow")
            .expect("workflow exists")
            .state,
        "paused"
    );
}

#[tokio::test]
async fn phase_migration_backfills_existing_workflow_nodes_with_an_empty_label() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("workflow-phase-migration.db");
    let pool = connect_sqlite(&format!("sqlite://{}", db_path.display()))
        .await
        .expect("connect");

    sqlx::raw_sql(include_str!(
        "../migrations/0010_add_workflow_orchestration.sql"
    ))
    .execute(&pool)
    .await
    .expect("create pre-phase workflow schema");
    sqlx::raw_sql(include_str!(
        "../migrations/0011_add_workflow_node_type.sql"
    ))
    .execute(&pool)
    .await
    .expect("add node type");
    sqlx::query(
        r#"INSERT INTO workflows (workflow_id, title, cwd, state)
           VALUES ('wf_existing', 'Existing', '/work', 'pending')"#,
    )
    .execute(&pool)
    .await
    .expect("insert existing workflow");
    sqlx::query(
        r#"INSERT INTO workflow_nodes
           (node_id, workflow_id, node_type, title, instructions, inputs, output)
           VALUES ('node_existing', 'wf_existing', 'agent', 'Existing node', 'Work', '[]', 'result.md')"#,
    )
    .execute(&pool)
    .await
    .expect("insert pre-phase node");

    sqlx::raw_sql(include_str!(
        "../migrations/0012_add_workflow_node_phase.sql"
    ))
    .execute(&pool)
    .await
    .expect("add phase");

    let phase: String =
        sqlx::query_scalar("SELECT phase FROM workflow_nodes WHERE node_id = 'node_existing'")
            .fetch_one(&pool)
            .await
            .expect("load migrated phase");
    assert_eq!(phase, "");
}
