use std::{
    future::Future,
    path::Path,
    sync::{Arc, Mutex},
};

use pontia_application::CreateSessionRequest;
use pontia_core::domain::{DomainEvent, EventSource, EventType};
use pontia_storage_sqlite::{
    connect_sqlite,
    repositories::workflows::{
        CreateWorkflowNodeRecord, CreateWorkflowRecord, SqliteWorkflowRepository,
    },
    run_migrations,
};
use pontia_workflow::{
    AgentEventSubscriber, GracefulExitRequester, SessionCreator, SubmitWorkflowNodeRequest,
    WorkflowScheduler,
};
use serde_json::json;
use tokio::sync::broadcast;

#[derive(Clone)]
struct SequencedSessionCreator {
    session_ids: Arc<Mutex<Vec<String>>>,
    requests: Arc<Mutex<Vec<CreateSessionRequest>>>,
}

impl SequencedSessionCreator {
    fn new(session_ids: &[&str]) -> Self {
        Self {
            session_ids: Arc::new(Mutex::new(
                session_ids.iter().map(|id| (*id).to_string()).collect(),
            )),
            requests: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl SessionCreator for SequencedSessionCreator {
    async fn create_session(
        &self,
        request: CreateSessionRequest,
    ) -> pontia_workflow::Result<String> {
        let index = {
            let mut requests = self.requests.lock().expect("session requests lock");
            let index = requests.len();
            requests.push(request);
            index
        };
        Ok(self.session_ids.lock().expect("session ids lock")[index].clone())
    }
}

#[derive(Clone, Default)]
struct RecordingExitRequester {
    requests: Arc<Mutex<Vec<(String, String)>>>,
}

impl GracefulExitRequester for RecordingExitRequester {
    fn ensure_current_runtime(
        &self,
        session_id: &str,
        runtime_instance_id: &str,
    ) -> impl Future<Output = pontia_workflow::Result<()>> + Send {
        let session_id = session_id.to_string();
        let runtime_instance_id = runtime_instance_id.to_string();
        async move {
            if runtime_instance_id == format!("runtime_{session_id}") {
                Ok(())
            } else {
                Err(pontia_workflow::Error::RuntimeMismatch {
                    session_id,
                    runtime_instance_id,
                })
            }
        }
    }

    fn request_graceful_exit(
        &self,
        session_id: &str,
        runtime_instance_id: &str,
    ) -> impl Future<Output = pontia_workflow::Result<()>> + Send {
        let requests = self.requests.clone();
        let session_id = session_id.to_string();
        let runtime_instance_id = runtime_instance_id.to_string();
        async move {
            requests
                .lock()
                .expect("exit requests lock")
                .push((session_id, runtime_instance_id));
            Ok(())
        }
    }
}

#[derive(Clone)]
struct TestAgentEvents {
    sender: broadcast::Sender<DomainEvent>,
}

impl TestAgentEvents {
    fn new() -> Self {
        Self::with_capacity(16)
    }

    fn with_capacity(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }

    fn publish_session_exit(&self, event_id: &str, session_id: &str) {
        self.sender
            .send(DomainEvent::new(
                event_id.to_string(),
                session_id.to_string(),
                None,
                EventSource::AgentClient,
                "pi".to_string(),
                EventType::SessionExited,
                json!({ "runtime_instance_id": format!("runtime_{session_id}") }),
            ))
            .expect("workflow event subscriber");
    }
}

impl AgentEventSubscriber for TestAgentEvents {
    fn subscribe(&self) -> broadcast::Receiver<DomainEvent> {
        self.sender.subscribe()
    }
}

async fn test_pool(path: &Path) -> sqlx::SqlitePool {
    let database_url = format!("sqlite://{}", path.display());
    let pool = connect_sqlite(&database_url).await.expect("connect");
    run_migrations(&pool).await.expect("migrate");
    pool
}

async fn wait_for_session(
    repository: &SqliteWorkflowRepository,
    node_id: &str,
    expected_session_id: &str,
) {
    for _ in 0..100 {
        let session_id = repository
            .get_node(node_id)
            .await
            .expect("load node")
            .expect("node exists")
            .session_id;
        if session_id.as_deref() == Some(expected_session_id) {
            return;
        }
        tokio::task::yield_now().await;
    }
    panic!("node {node_id} did not bind session {expected_session_id}");
}

async fn wait_for_completed(repository: &SqliteWorkflowRepository, workflow_id: &str) {
    for _ in 0..100 {
        let state = repository
            .get_workflow(workflow_id)
            .await
            .expect("load workflow")
            .expect("workflow exists")
            .state;
        if state == "completed" {
            return;
        }
        tokio::task::yield_now().await;
    }
    panic!("workflow did not complete");
}

#[tokio::test]
async fn confirmed_exits_chain_three_agent_nodes_with_declared_handoff_inputs() {
    let temp = tempfile::tempdir().expect("tempdir");
    let pool = test_pool(&temp.path().join("chain.db")).await;
    let repository = SqliteWorkflowRepository::new(pool.clone());
    repository
        .create_workflow(CreateWorkflowRecord {
            workflow_id: "wf_chain".to_string(),
            title: "Three node workflow".to_string(),
            cwd: "/workspace/shared-project".to_string(),
            state: "pending".to_string(),
        })
        .await
        .expect("create workflow");

    for node in [
        CreateWorkflowNodeRecord {
            node_id: "node_research".to_string(),
            workflow_id: "wf_chain".to_string(),
            parent_node_id: None,
            title: "Researcher".to_string(),
            instructions: "Produce the research handoff.".to_string(),
            inputs: json!(["brief.md"]).to_string(),
            output: "research.md".to_string(),
            execution_profile_id: Some("research".to_string()),
            execution_profile_version: Some("1".to_string()),
        },
        CreateWorkflowNodeRecord {
            node_id: "node_draft".to_string(),
            workflow_id: "wf_chain".to_string(),
            parent_node_id: Some("node_research".to_string()),
            title: "Drafter".to_string(),
            instructions: "Draft from all declared evidence.".to_string(),
            inputs: json!(["research.md", "style.md"]).to_string(),
            output: "draft.md".to_string(),
            execution_profile_id: Some("writer".to_string()),
            execution_profile_version: Some("2".to_string()),
        },
        CreateWorkflowNodeRecord {
            node_id: "node_review".to_string(),
            workflow_id: "wf_chain".to_string(),
            parent_node_id: Some("node_draft".to_string()),
            title: "Reviewer".to_string(),
            instructions: "Review only the declared checklist.".to_string(),
            inputs: json!(["checklist.md"]).to_string(),
            output: "review.md".to_string(),
            execution_profile_id: Some("review".to_string()),
            execution_profile_version: Some("3".to_string()),
        },
    ] {
        repository.create_node(node).await.expect("create node");
    }

    let pontia_home = temp.path().join("pontia-home");
    let handoff = pontia_home.join("workflows/wf_chain/handoff");
    std::fs::create_dir_all(&handoff).expect("create handoff fixtures");
    std::fs::write(handoff.join("brief.md"), "需求：调查工作流。").expect("write initial brief");
    std::fs::write(handoff.join("style.md"), "Use a compact style.")
        .expect("write independent style input");
    std::fs::write(handoff.join("checklist.md"), "检查：引用与结论。")
        .expect("write independent checklist input");

    let sessions =
        SequencedSessionCreator::new(&["session_research", "session_draft", "session_review"]);
    let exits = RecordingExitRequester::default();
    let events = TestAgentEvents::new();
    let scheduler = WorkflowScheduler::with_services(
        pool,
        sessions.clone(),
        exits.clone(),
        events.clone(),
        pontia_home,
    );

    scheduler.start("wf_chain").await.expect("start workflow");
    assert_eq!(sessions.requests.lock().expect("requests lock").len(), 1);
    assert_eq!(
        repository
            .get_node("node_draft")
            .await
            .expect("load draft node")
            .expect("draft node exists")
            .session_id,
        None
    );

    scheduler
        .submit(SubmitWorkflowNodeRequest {
            session_id: "session_research".to_string(),
            runtime_instance_id: "runtime_session_research".to_string(),
            output: "research.md".to_string(),
            content: "研究结果：真实提交内容。".to_string(),
        })
        .await
        .expect("submit research");
    tokio::task::yield_now().await;
    assert_eq!(sessions.requests.lock().expect("requests lock").len(), 1);

    events.publish_session_exit("evt_research_exit", "session_research");
    wait_for_session(&repository, "node_draft", "session_draft").await;
    assert_eq!(sessions.requests.lock().expect("requests lock").len(), 2);

    scheduler
        .submit(SubmitWorkflowNodeRequest {
            session_id: "session_draft".to_string(),
            runtime_instance_id: "runtime_session_draft".to_string(),
            output: "draft.md".to_string(),
            content: "Adjacent draft content must not be inferred by the reviewer.".to_string(),
        })
        .await
        .expect("submit draft");
    tokio::task::yield_now().await;
    assert_eq!(sessions.requests.lock().expect("requests lock").len(), 2);

    events.publish_session_exit("evt_draft_exit", "session_draft");
    wait_for_session(&repository, "node_review", "session_review").await;

    let requests = sessions.requests.lock().expect("requests lock").clone();
    assert_eq!(requests.len(), 3);
    assert_eq!(
        requests
            .iter()
            .map(|request| request.workspace.as_deref())
            .collect::<Vec<_>>(),
        vec![
            Some("/workspace/shared-project"),
            Some("/workspace/shared-project"),
            Some("/workspace/shared-project"),
        ]
    );
    assert_eq!(
        requests
            .iter()
            .map(|request| request.title.as_deref())
            .collect::<Vec<_>>(),
        vec![Some("Researcher"), Some("Drafter"), Some("Reviewer")]
    );
    assert_eq!(
        requests
            .iter()
            .map(|request| request.execution_profile_id.as_deref())
            .collect::<Vec<_>>(),
        vec![Some("research"), Some("writer"), Some("review")]
    );
    assert_eq!(
        requests
            .iter()
            .map(|request| request.execution_profile_version.as_deref())
            .collect::<Vec<_>>(),
        vec![Some("1"), Some("2"), Some("3")]
    );
    let draft_task = &requests[1]
        .initial_task
        .as_ref()
        .expect("draft initial task")
        .input;
    assert!(draft_task.contains("Draft from all declared evidence."));
    assert!(draft_task.contains("研究结果：真实提交内容。"));
    assert!(draft_task.contains("Use a compact style."));
    let review_task = &requests[2]
        .initial_task
        .as_ref()
        .expect("review initial task")
        .input;
    assert!(review_task.contains("Review only the declared checklist."));
    assert!(review_task.contains("检查：引用与结论。"));
    assert!(!review_task.contains("Adjacent draft content"));
    assert!(!review_task.contains("Input file: draft.md"));

    scheduler
        .submit(SubmitWorkflowNodeRequest {
            session_id: "session_review".to_string(),
            runtime_instance_id: "runtime_session_review".to_string(),
            output: "review.md".to_string(),
            content: "Review complete.".to_string(),
        })
        .await
        .expect("submit review");
    assert_eq!(
        repository
            .get_workflow("wf_chain")
            .await
            .expect("load workflow")
            .expect("workflow exists")
            .state,
        "running"
    );

    events.publish_session_exit("evt_review_exit", "session_review");
    events.publish_session_exit("evt_review_exit_duplicate", "session_review");
    wait_for_completed(&repository, "wf_chain").await;

    let mut session_ids = Vec::new();
    for node_id in ["node_research", "node_draft", "node_review"] {
        session_ids.push(
            repository
                .get_node(node_id)
                .await
                .expect("load chained node")
                .expect("chained node exists")
                .session_id,
        );
    }
    assert_eq!(
        session_ids,
        vec![
            Some("session_research".to_string()),
            Some("session_draft".to_string()),
            Some("session_review".to_string()),
        ]
    );
    let workflow_events = repository
        .list_events("wf_chain")
        .await
        .expect("list workflow events");
    assert_eq!(
        workflow_events
            .iter()
            .map(|event| event.event_type.as_str())
            .collect::<Vec<_>>(),
        vec!["workflow.started", "workflow.completed"]
    );
    assert_eq!(exits.requests.lock().expect("exit requests lock").len(), 3);
}

#[tokio::test]
async fn lagged_notifications_reconcile_a_persisted_confirmed_session_exit() {
    let temp = tempfile::tempdir().expect("tempdir");
    let pool = test_pool(&temp.path().join("lagged.db")).await;
    let repository = SqliteWorkflowRepository::new(pool.clone());
    repository
        .create_workflow(CreateWorkflowRecord {
            workflow_id: "wf_lagged".to_string(),
            title: "Lag recovery workflow".to_string(),
            cwd: "/workspace/shared-project".to_string(),
            state: "pending".to_string(),
        })
        .await
        .expect("create workflow");
    repository
        .create_node(CreateWorkflowNodeRecord {
            node_id: "node_lagged".to_string(),
            workflow_id: "wf_lagged".to_string(),
            parent_node_id: None,
            title: "Lagged worker".to_string(),
            instructions: "Produce the result.".to_string(),
            inputs: "[]".to_string(),
            output: "result.md".to_string(),
            execution_profile_id: None,
            execution_profile_version: None,
        })
        .await
        .expect("create node");

    let sessions = SequencedSessionCreator::new(&["session_lagged"]);
    let events = TestAgentEvents::with_capacity(1);
    let scheduler = WorkflowScheduler::with_services(
        pool.clone(),
        sessions,
        RecordingExitRequester::default(),
        events.clone(),
        temp.path().join("pontia-home"),
    );
    scheduler.start("wf_lagged").await.expect("start workflow");
    scheduler
        .submit(SubmitWorkflowNodeRequest {
            session_id: "session_lagged".to_string(),
            runtime_instance_id: "runtime_session_lagged".to_string(),
            output: "result.md".to_string(),
            content: "Done.".to_string(),
        })
        .await
        .expect("submit output");

    sqlx::query(
        r#"INSERT INTO events
           (event_id, session_id, source, client_type, event_type, occurred_at, payload)
           VALUES ('evt_lagged_exit', 'session_lagged', 'agent_client', 'pi',
                   'session.exited', '2026-07-31T00:00:00Z', '{}')"#,
    )
    .execute(&pool)
    .await
    .expect("persist confirmed Session exit fixture");
    events.publish_session_exit("evt_lagged_exit", "session_lagged");
    events.publish_session_exit("evt_overwriting_notification", "session_other");

    wait_for_completed(&repository, "wf_lagged").await;
}
