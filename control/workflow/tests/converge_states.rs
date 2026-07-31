use std::{
    collections::VecDeque,
    future::Future,
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

use pontia_application::CreateSessionRequest;
use pontia_core::{
    Error as PontiaError,
    domain::{DomainEvent, EventSource, EventType},
};
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
    outcomes: Arc<Mutex<VecDeque<Option<String>>>>,
    requests: Arc<Mutex<Vec<CreateSessionRequest>>>,
}

impl SequencedSessionCreator {
    fn new(outcomes: impl IntoIterator<Item = Option<&'static str>>) -> Self {
        Self {
            outcomes: Arc::new(Mutex::new(
                outcomes
                    .into_iter()
                    .map(|outcome| outcome.map(str::to_string))
                    .collect(),
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
        self.requests
            .lock()
            .expect("session requests lock")
            .push(request);
        self.outcomes
            .lock()
            .expect("session outcomes lock")
            .pop_front()
            .expect("configured Session outcome")
            .ok_or_else(|| std::io::Error::other("configured Session creation failure").into())
    }
}

#[derive(Clone, Default)]
struct RecordingExitRequester {
    ensure_missing_binding: bool,
    request_error: Arc<Mutex<Option<PontiaError>>>,
    requests: Arc<Mutex<Vec<(String, String)>>>,
}

impl RecordingExitRequester {
    fn missing_runtime_binding() -> Self {
        Self {
            ensure_missing_binding: true,
            ..Self::default()
        }
    }

    fn failing_request() -> Self {
        Self {
            request_error: Arc::new(Mutex::new(Some(PontiaError::Io(std::io::Error::other(
                "configured graceful exit failure",
            ))))),
            ..Self::default()
        }
    }
}

impl GracefulExitRequester for RecordingExitRequester {
    fn ensure_current_runtime(
        &self,
        _session_id: &str,
        _runtime_instance_id: &str,
    ) -> impl Future<Output = pontia_workflow::Result<()>> + Send {
        let missing = self.ensure_missing_binding;
        let session_id = _session_id.to_string();
        async move {
            if missing {
                Err(pontia_workflow::Error::RuntimeControlUnavailable {
                    session_id,
                    message: "no current runtime binding".to_string(),
                })
            } else {
                Ok(())
            }
        }
    }

    fn request_graceful_exit(
        &self,
        session_id: &str,
        runtime_instance_id: &str,
    ) -> impl Future<Output = pontia_workflow::Result<()>> + Send {
        self.requests
            .lock()
            .expect("exit requests lock")
            .push((session_id.to_string(), runtime_instance_id.to_string()));
        let error = self
            .request_error
            .lock()
            .expect("request error lock")
            .take();
        async move { error.map_or(Ok(()), |error| Err(error.into())) }
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

    fn publish(&self, session_id: &str, event_type: EventType) {
        self.sender
            .send(DomainEvent::new(
                format!("evt_{event_type}"),
                session_id.to_string(),
                Some("turn_root".to_string()),
                EventSource::AgentClient,
                "pi".to_string(),
                event_type,
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

async fn seed_linear_workflow(
    repository: &SqliteWorkflowRepository,
    workflow_id: &str,
    root_inputs: &str,
    with_child: bool,
) {
    repository
        .create_workflow(CreateWorkflowRecord {
            workflow_id: workflow_id.to_string(),
            title: "Convergence workflow".to_string(),
            cwd: "/workspace/project".to_string(),
            state: "pending".to_string(),
        })
        .await
        .expect("create workflow");
    repository
        .create_node(CreateWorkflowNodeRecord {
            node_id: format!("{workflow_id}_root"),
            workflow_id: workflow_id.to_string(),
            parent_node_id: None,
            title: "Root".to_string(),
            instructions: "Produce the root output.".to_string(),
            inputs: root_inputs.to_string(),
            output: "root.md".to_string(),
            execution_profile_id: None,
            execution_profile_version: None,
        })
        .await
        .expect("create root node");
    if with_child {
        repository
            .create_node(CreateWorkflowNodeRecord {
                node_id: format!("{workflow_id}_child"),
                workflow_id: workflow_id.to_string(),
                parent_node_id: Some(format!("{workflow_id}_root")),
                title: "Child".to_string(),
                instructions: "Produce the child output.".to_string(),
                inputs: "[\"root.md\"]".to_string(),
                output: "child.md".to_string(),
                execution_profile_id: None,
                execution_profile_version: None,
            })
            .await
            .expect("create child node");
    }
}

async fn wait_for_state(repository: &SqliteWorkflowRepository, workflow_id: &str, expected: &str) {
    for _ in 0..200 {
        let workflow = repository
            .get_workflow(workflow_id)
            .await
            .expect("load workflow")
            .expect("workflow exists");
        if workflow.state == expected {
            return;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("workflow {workflow_id} did not reach {expected}");
}

async fn assert_transition(
    repository: &SqliteWorkflowRepository,
    workflow_id: &str,
    expected_state: &str,
    expected_event: &str,
) -> Option<String> {
    let workflow = repository
        .get_workflow(workflow_id)
        .await
        .expect("load workflow")
        .expect("workflow exists");
    assert_eq!(workflow.state, expected_state);
    let events = repository
        .list_events(workflow_id)
        .await
        .expect("list workflow events");
    assert_eq!(events.len(), 2);
    assert_eq!(events[1].event_type, expected_event);
    workflow.failure_message
}

#[tokio::test]
async fn unsubmitted_completed_turn_enters_idle_and_keeps_the_current_session() {
    let temp = tempfile::tempdir().expect("tempdir");
    let pool = test_pool(&temp.path().join("idle.db")).await;
    let repository = SqliteWorkflowRepository::new(pool.clone());
    seed_linear_workflow(&repository, "wf_idle", "[]", true).await;
    let sessions = SequencedSessionCreator::new([Some("session_root"), Some("session_child")]);
    let exits = RecordingExitRequester::default();
    let events = TestAgentEvents::new();
    let scheduler = WorkflowScheduler::with_services(
        pool,
        sessions.clone(),
        exits.clone(),
        events.clone(),
        temp.path().join("pontia-home"),
    );
    scheduler.start("wf_idle").await.expect("start workflow");

    events.publish("session_root", EventType::TurnCompleted);
    wait_for_state(&repository, "wf_idle", "idle").await;

    assert_transition(&repository, "wf_idle", "idle", "workflow.idle").await;
    assert!(
        exits
            .requests
            .lock()
            .expect("exit requests lock")
            .is_empty()
    );
    assert_eq!(
        sessions
            .requests
            .lock()
            .expect("session requests lock")
            .len(),
        1
    );
    assert_eq!(
        repository
            .get_node("wf_idle_root")
            .await
            .expect("load root")
            .expect("root exists")
            .session_id
            .as_deref(),
        Some("session_root")
    );
}

#[tokio::test]
async fn unsubmitted_failure_facts_fail_once_cleanup_once_and_never_start_a_child() {
    for (event_type, expected_message, expects_cleanup) in [
        (EventType::TurnFailed, "turn.failed", true),
        (EventType::TurnInterrupted, "turn.interrupted", true),
        (EventType::SessionExited, "session.exited", false),
    ] {
        let temp = tempfile::tempdir().expect("tempdir");
        let pool = test_pool(&temp.path().join("failure.db")).await;
        let repository = SqliteWorkflowRepository::new(pool.clone());
        seed_linear_workflow(&repository, "wf_failure", "[]", true).await;
        let sessions = SequencedSessionCreator::new([Some("session_root"), Some("session_child")]);
        let exits = RecordingExitRequester::default();
        let events = TestAgentEvents::new();
        let scheduler = WorkflowScheduler::with_services(
            pool,
            sessions.clone(),
            exits.clone(),
            events.clone(),
            temp.path().join("pontia-home"),
        );
        scheduler.start("wf_failure").await.expect("start workflow");

        events.publish("session_root", event_type);
        events.publish("session_root", event_type);
        wait_for_state(&repository, "wf_failure", "failed").await;

        let failure_message =
            assert_transition(&repository, "wf_failure", "failed", "workflow.failed")
                .await
                .expect("failure message");
        assert!(
            failure_message.contains(expected_message),
            "{failure_message}"
        );
        assert_eq!(
            exits.requests.lock().expect("exit requests lock").len(),
            usize::from(expects_cleanup)
        );
        assert_eq!(
            sessions
                .requests
                .lock()
                .expect("session requests lock")
                .len(),
            1
        );
    }
}

#[tokio::test]
async fn activation_errors_fail_with_clear_messages() {
    for (workflow_id, input_setup, outcomes, expected_message) in [
        (
            "wf_missing_input",
            "missing",
            vec![Some("session_unused")],
            "missing.md",
        ),
        (
            "wf_non_utf8_input",
            "non_utf8",
            vec![Some("session_unused")],
            "UTF-8",
        ),
        (
            "wf_session_creation",
            "none",
            vec![None],
            "Session creation",
        ),
    ] {
        let temp = tempfile::tempdir().expect("tempdir");
        let pool = test_pool(&temp.path().join(format!("{workflow_id}.db"))).await;
        let repository = SqliteWorkflowRepository::new(pool.clone());
        let inputs = if input_setup == "none" {
            "[]"
        } else {
            "[\"missing.md\"]"
        };
        seed_linear_workflow(&repository, workflow_id, inputs, false).await;
        let handoff_dir = temp
            .path()
            .join("pontia-home/workflows")
            .join(workflow_id)
            .join("handoff");
        std::fs::create_dir_all(&handoff_dir).expect("create handoff dir");
        if input_setup == "non_utf8" {
            std::fs::write(handoff_dir.join("missing.md"), [0xff, 0xfe])
                .expect("write non-UTF-8 input");
        }
        let scheduler = WorkflowScheduler::with_services(
            pool,
            SequencedSessionCreator::new(outcomes),
            RecordingExitRequester::default(),
            TestAgentEvents::new(),
            temp.path().join("pontia-home"),
        );

        let error = scheduler
            .start(workflow_id)
            .await
            .expect_err("activation must fail");
        let failure_message =
            assert_transition(&repository, workflow_id, "failed", "workflow.failed")
                .await
                .expect("failure message");
        assert!(
            failure_message.contains(expected_message),
            "{failure_message}; {error}"
        );
    }
}

#[tokio::test]
async fn submission_binding_and_exit_failures_fail_without_starting_a_child() {
    for (workflow_id, exits, expected_message, expected_exit_requests) in [
        (
            "wf_missing_binding",
            RecordingExitRequester::missing_runtime_binding(),
            "runtime binding",
            0,
        ),
        (
            "wf_exit_failure",
            RecordingExitRequester::failing_request(),
            "graceful exit",
            1,
        ),
    ] {
        let temp = tempfile::tempdir().expect("tempdir");
        let pool = test_pool(&temp.path().join(format!("{workflow_id}.db"))).await;
        let repository = SqliteWorkflowRepository::new(pool.clone());
        seed_linear_workflow(&repository, workflow_id, "[]", true).await;
        let sessions = SequencedSessionCreator::new([Some("session_root"), Some("session_child")]);
        let scheduler_task_owner = Arc::downgrade(&sessions.requests);
        let events = TestAgentEvents::new();
        let scheduler = WorkflowScheduler::with_services(
            pool,
            sessions.clone(),
            exits.clone(),
            events.clone(),
            temp.path().join("pontia-home"),
        );
        scheduler.start(workflow_id).await.expect("start workflow");

        let error = scheduler
            .submit(SubmitWorkflowNodeRequest {
                session_id: "session_root".to_string(),
                runtime_instance_id: "runtime_session_root".to_string(),
                output: "root.md".to_string(),
                content: "root output".to_string(),
            })
            .await
            .expect_err("submission orchestration must fail");

        let failure_message =
            assert_transition(&repository, workflow_id, "failed", "workflow.failed")
                .await
                .expect("failure message");
        assert!(
            failure_message.contains(expected_message),
            "{failure_message}; {error}"
        );
        assert_eq!(
            sessions
                .requests
                .lock()
                .expect("session requests lock")
                .len(),
            1
        );
        assert_eq!(
            exits.requests.lock().expect("exit requests lock").len(),
            expected_exit_requests
        );
        assert!(
            repository
                .get_node(&format!("{workflow_id}_child"))
                .await
                .expect("load child")
                .expect("child exists")
                .session_id
                .is_none()
        );
        drop(scheduler);
        drop(sessions);
        tokio::time::timeout(Duration::from_secs(2), async {
            while scheduler_task_owner.upgrade().is_some() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("failed Workflow Scheduler task should end without another Agent fact");
    }
}

#[tokio::test]
async fn downstream_session_creation_failure_stops_the_workflow() {
    let temp = tempfile::tempdir().expect("tempdir");
    let pool = test_pool(&temp.path().join("downstream-failure.db")).await;
    let repository = SqliteWorkflowRepository::new(pool.clone());
    seed_linear_workflow(&repository, "wf_downstream_failure", "[]", true).await;
    let sessions = SequencedSessionCreator::new([Some("session_root"), None]);
    let exits = RecordingExitRequester::default();
    let events = TestAgentEvents::new();
    let scheduler = WorkflowScheduler::with_services(
        pool,
        sessions.clone(),
        exits.clone(),
        events.clone(),
        temp.path().join("pontia-home"),
    );
    scheduler
        .start("wf_downstream_failure")
        .await
        .expect("start workflow");
    scheduler
        .submit(SubmitWorkflowNodeRequest {
            session_id: "session_root".to_string(),
            runtime_instance_id: "runtime_session_root".to_string(),
            output: "root.md".to_string(),
            content: "root output".to_string(),
        })
        .await
        .expect("submit root output");

    events.publish("session_root", EventType::SessionExited);
    wait_for_state(&repository, "wf_downstream_failure", "failed").await;

    let failure_message = assert_transition(
        &repository,
        "wf_downstream_failure",
        "failed",
        "workflow.failed",
    )
    .await
    .expect("failure message");
    assert!(
        failure_message.contains("Session creation"),
        "{failure_message}"
    );
    assert_eq!(
        sessions
            .requests
            .lock()
            .expect("session requests lock")
            .len(),
        2
    );
    assert_eq!(exits.requests.lock().expect("exit requests lock").len(), 1);
    assert!(
        repository
            .get_node("wf_downstream_failure_child")
            .await
            .expect("load child")
            .expect("child exists")
            .session_id
            .is_none()
    );
}

#[tokio::test]
async fn submission_and_terminal_transition_have_one_atomic_winner() {
    let temp = tempfile::tempdir().expect("tempdir");
    let pool = test_pool(&temp.path().join("atomic-race.db")).await;
    let repository = SqliteWorkflowRepository::new(pool);

    for index in 0..20 {
        let workflow_id = format!("wf_atomic_{index}");
        let node_id = format!("{workflow_id}_root");
        seed_linear_workflow(&repository, &workflow_id, "[]", false).await;
        repository
            .start_workflow(&workflow_id, &format!("evt_started_{index}"))
            .await
            .expect("start workflow record");

        let submission_repository = repository.clone();
        let submission_node_id = node_id.clone();
        let terminal_repository = repository.clone();
        let terminal_workflow_id = workflow_id.clone();
        let terminal_node_id = node_id.clone();
        let (submission, terminal) = tokio::join!(
            async move {
                submission_repository
                    .record_node_submission(&submission_node_id)
                    .await
            },
            async move {
                terminal_repository
                    .idle_unsubmitted_workflow_node(
                        &terminal_workflow_id,
                        &terminal_node_id,
                        &format!("evt_idle_{index}"),
                    )
                    .await
            }
        );

        assert_ne!(submission.is_ok(), terminal.is_ok());
        let workflow = repository
            .get_workflow(&workflow_id)
            .await
            .expect("load workflow")
            .expect("workflow exists");
        let node = repository
            .get_node(&node_id)
            .await
            .expect("load node")
            .expect("node exists");
        if submission.is_ok() {
            assert_eq!(workflow.state, "running");
            assert!(node.submitted_at.is_some());
        } else {
            assert_eq!(workflow.state, "idle");
            assert!(node.submitted_at.is_none());
        }
    }
}

#[tokio::test]
async fn lagged_notifications_reconcile_persisted_turn_terminal_facts() {
    for (event_type, expected_state) in [
        (EventType::TurnCompleted, "idle"),
        (EventType::TurnFailed, "failed"),
        (EventType::TurnInterrupted, "failed"),
    ] {
        let temp = tempfile::tempdir().expect("tempdir");
        let pool = test_pool(&temp.path().join("lagged-terminal.db")).await;
        let repository = SqliteWorkflowRepository::new(pool.clone());
        seed_linear_workflow(&repository, "wf_lagged_terminal", "[]", false).await;
        let events = TestAgentEvents::with_capacity(1);
        let scheduler = WorkflowScheduler::with_services(
            pool.clone(),
            SequencedSessionCreator::new([Some("session_root")]),
            RecordingExitRequester::default(),
            events.clone(),
            temp.path().join("pontia-home"),
        );
        scheduler
            .start("wf_lagged_terminal")
            .await
            .expect("start workflow");

        sqlx::query(
            r#"INSERT INTO events
               (event_id, session_id, turn_id, source, client_type, event_type, occurred_at, payload)
               VALUES (?, 'session_root', 'turn_root', 'agent_client', 'pi', ?,
                       '2026-07-31T00:00:00Z',
                       '{"runtime_instance_id":"runtime_session_root"}')"#,
        )
        .bind(format!("evt_persisted_{event_type}"))
        .bind(event_type.to_string())
        .execute(&pool)
        .await
        .expect("persist terminal Agent fact");
        events.publish("session_root", event_type);
        events.publish("session_other", EventType::TurnCompleted);

        wait_for_state(&repository, "wf_lagged_terminal", expected_state).await;
    }
}
