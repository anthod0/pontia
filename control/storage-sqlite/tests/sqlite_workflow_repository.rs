use pontia_storage_sqlite::{
    connect_sqlite,
    repositories::workflows::{
        CreateWorkflowNodeRecord, CreateWorkflowRecord, SqliteWorkflowRepository,
    },
    run_migrations,
};
use serde_json::json;

async fn test_pool() -> sqlx::SqlitePool {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("sqlite_workflow_repository.db");
    let _kept_dir = dir.keep();
    let database_url = format!("sqlite://{}", db_path.display());
    let pool = connect_sqlite(&database_url).await.expect("connect");
    run_migrations(&pool).await.expect("migrate");
    pool
}

#[tokio::test]
async fn repository_persists_workflow_nodes_bindings_submissions_and_ordered_events() {
    let pool = test_pool().await;
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
        .bind_node_session("node_1", "session_1")
        .await
        .expect("bind session");
    repository
        .record_node_submission("node_1")
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
    assert_eq!(workflow.state, "pending");

    let nodes = repository.list_nodes("wf_1").await.expect("list nodes");
    assert_eq!(nodes.len(), 2);
    assert_eq!(nodes[0].node_id, "node_1");
    assert_eq!(nodes[0].inputs, r#"["brief.md","changes.md"]"#);
    assert_eq!(nodes[0].session_id.as_deref(), Some("session_1"));
    assert!(nodes[0].submitted_at.is_some());
    assert_eq!(nodes[1].parent_node_id.as_deref(), Some("node_1"));
    assert_eq!(nodes[1].output, "approved.md");

    let events = repository.list_events("wf_1").await.expect("list events");
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].sequence, 1);
    assert_eq!(events[0].event_id, "evt_2");
    assert_eq!(events[1].sequence, 2);
    assert_eq!(events[1].event_id, "evt_3");
}

#[tokio::test]
async fn starting_a_pending_workflow_atomically_updates_state_and_appends_started_event() {
    let repository = SqliteWorkflowRepository::new(test_pool().await);
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
    let repository = SqliteWorkflowRepository::new(test_pool().await);
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
