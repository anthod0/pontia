use std::{
    path::Path,
    sync::{Arc, Mutex},
};

use pontia_application::{AgentEventBroker, CreateSessionRequest};
use pontia_storage_sqlite::{
    connect_sqlite,
    repositories::workflows::{
        CreateWorkflowNodeRecord, CreateWorkflowRecord, SqliteWorkflowRepository,
    },
    run_migrations,
};
use pontia_workflow::{SessionCreator, WorkflowScheduler};
use serde_json::json;

#[derive(Clone, Default)]
struct RecordingSessionCreator {
    requests: Arc<Mutex<Vec<CreateSessionRequest>>>,
}

impl SessionCreator for RecordingSessionCreator {
    async fn create_session(
        &self,
        request: CreateSessionRequest,
    ) -> pontia_workflow::Result<String> {
        self.requests.lock().expect("requests lock").push(request);
        Ok("session_workflow_1".to_string())
    }
}

async fn test_pool(path: &Path) -> sqlx::SqlitePool {
    let database_url = format!("sqlite://{}", path.display());
    let pool = connect_sqlite(&database_url).await.expect("connect");
    run_migrations(&pool).await.expect("migrate");
    pool
}

#[tokio::test]
async fn start_launches_first_node_as_a_pi_session_with_handoff_protocol() {
    let temp = tempfile::tempdir().expect("tempdir");
    let pool = test_pool(&temp.path().join("workflow.db")).await;
    let repository = SqliteWorkflowRepository::new(pool.clone());
    repository
        .create_workflow(CreateWorkflowRecord {
            workflow_id: "wf_start".to_string(),
            title: "Release workflow".to_string(),
            cwd: "/workspace/project".to_string(),
            state: "pending".to_string(),
        })
        .await
        .expect("create workflow");
    repository
        .create_node(CreateWorkflowNodeRecord {
            node_id: "node_writer".to_string(),
            workflow_id: "wf_start".to_string(),
            parent_node_id: None,
            title: "Release writer".to_string(),
            instructions: "Turn the brief into concise release notes.".to_string(),
            inputs: json!(["brief.md"]).to_string(),
            output: "release.md".to_string(),
            execution_profile_id: Some("writer".to_string()),
            execution_profile_version: Some("3".to_string()),
        })
        .await
        .expect("create node");

    let pontia_home = temp.path().join("pontia-home");
    let handoff = pontia_home.join("workflows/wf_start/handoff");
    std::fs::create_dir_all(&handoff).expect("create handoff fixtures");
    std::fs::write(
        handoff.join("brief.md"),
        "Ship the workflow scheduler.\nKeep the notes short.",
    )
    .expect("write handoff input");
    let sessions = RecordingSessionCreator::default();
    let scheduler = WorkflowScheduler::new(
        pool,
        sessions.clone(),
        AgentEventBroker::default(),
        pontia_home,
    );

    let outcome = scheduler.start("wf_start").await.expect("start workflow");

    assert_eq!(outcome.node_id, "node_writer");
    assert_eq!(outcome.session_id, "session_workflow_1");
    let request = {
        let requests = sessions.requests.lock().expect("requests lock");
        assert_eq!(requests.len(), 1);
        requests[0].clone()
    };
    assert_eq!(request.client_type, "pi");
    assert_eq!(request.title.as_deref(), Some("Release writer"));
    assert_eq!(request.workspace.as_deref(), Some("/workspace/project"));
    assert_eq!(request.workspace_id, None);
    assert_eq!(request.execution_profile_id.as_deref(), Some("writer"));
    assert_eq!(request.execution_profile_version.as_deref(), Some("3"));
    let task = &request.initial_task.as_ref().expect("initial task").input;
    assert!(task.contains("Turn the brief into concise release notes."));
    assert!(task.contains("Input file: brief.md"));
    assert!(task.contains("Ship the workflow scheduler.\nKeep the notes short."));
    assert!(task.contains("Expected output: release.md"));
    assert!(task.contains("create a source file in the Session cwd"));
    assert!(task.contains("pontiactl workflow submit --input <source-path> --output release.md"));

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
        .expect("list workflow events");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, "workflow.started");
    let node = repository
        .get_node("node_writer")
        .await
        .expect("load node")
        .expect("node exists");
    assert_eq!(node.session_id.as_deref(), Some("session_workflow_1"));
}

#[tokio::test]
async fn start_creates_the_workflow_handoff_directory() {
    let temp = tempfile::tempdir().expect("tempdir");
    let pool = test_pool(&temp.path().join("workflow-directory.db")).await;
    let repository = SqliteWorkflowRepository::new(pool.clone());
    repository
        .create_workflow(CreateWorkflowRecord {
            workflow_id: "wf_directory".to_string(),
            title: "Directory workflow".to_string(),
            cwd: "/workspace/project".to_string(),
            state: "pending".to_string(),
        })
        .await
        .expect("create workflow");
    repository
        .create_node(CreateWorkflowNodeRecord {
            node_id: "node_no_inputs".to_string(),
            workflow_id: "wf_directory".to_string(),
            parent_node_id: None,
            title: "Standalone writer".to_string(),
            instructions: "Write a standalone result.".to_string(),
            inputs: "[]".to_string(),
            output: "result.md".to_string(),
            execution_profile_id: None,
            execution_profile_version: None,
        })
        .await
        .expect("create node");
    let pontia_home = temp.path().join("new-pontia-home");
    let scheduler = WorkflowScheduler::new(
        pool,
        RecordingSessionCreator::default(),
        AgentEventBroker::default(),
        pontia_home.clone(),
    );

    scheduler
        .start("wf_directory")
        .await
        .expect("start workflow");

    assert!(pontia_home.join("workflows/wf_directory/handoff").is_dir());
}

#[tokio::test]
async fn start_rejects_handoff_input_names_that_escape_the_handoff_directory() {
    let temp = tempfile::tempdir().expect("tempdir");
    let pool = test_pool(&temp.path().join("workflow-paths.db")).await;
    let repository = SqliteWorkflowRepository::new(pool.clone());
    let pontia_home = temp.path().join("pontia-home");
    let secret = temp.path().join("outside.md");
    std::fs::write(&secret, "must not be read").expect("write outside file");

    for (suffix, input) in [
        ("parent", "../outside.md".to_string()),
        ("absolute", secret.display().to_string()),
    ] {
        let workflow_id = format!("wf_{suffix}");
        repository
            .create_workflow(CreateWorkflowRecord {
                workflow_id: workflow_id.clone(),
                title: "Unsafe input workflow".to_string(),
                cwd: "/workspace/project".to_string(),
                state: "pending".to_string(),
            })
            .await
            .expect("create workflow");
        repository
            .create_node(CreateWorkflowNodeRecord {
                node_id: format!("node_{suffix}"),
                workflow_id: workflow_id.clone(),
                parent_node_id: None,
                title: "Reader".to_string(),
                instructions: "Read the input.".to_string(),
                inputs: json!([input]).to_string(),
                output: "result.md".to_string(),
                execution_profile_id: None,
                execution_profile_version: None,
            })
            .await
            .expect("create node");
        let sessions = RecordingSessionCreator::default();
        let scheduler = WorkflowScheduler::new(
            pool.clone(),
            sessions.clone(),
            AgentEventBroker::default(),
            pontia_home.clone(),
        );

        let error = scheduler
            .start(&workflow_id)
            .await
            .expect_err("escaping handoff path must be rejected");

        assert!(error.to_string().contains("invalid Handoff file name"));
        assert!(sessions.requests.lock().expect("requests lock").is_empty());
    }
}
