use std::{
    future::Future,
    path::Path,
    sync::{Arc, Mutex},
};

use pontia_application::CreateSessionRequest;
use pontia_core::domain::{DomainEvent, EventSource, EventType};
use pontia_storage_sqlite::{
    connect_sqlite,
    repositories::{
        runtime_bindings::{RuntimeBindingUpsertRecord, SqliteRuntimeBindingRepository},
        workflows::{CreateWorkflowNodeRecord, CreateWorkflowRecord, SqliteWorkflowRepository},
    },
    run_migrations,
};
use pontia_workflow::{
    AgentEventSubscriber, GracefulExitRequester, SessionCreator, SubmitWorkflowNodeRequest,
    WorkflowCoordinator, WorkflowScheduler,
};
use serde_json::json;
use tokio::sync::broadcast;

#[derive(Clone)]
struct BoundSessionCreator {
    pool: sqlx::SqlitePool,
    session_id: String,
    runtime_instance_id: String,
}

impl SessionCreator for BoundSessionCreator {
    async fn create_session(
        &self,
        _request: CreateSessionRequest,
    ) -> pontia_workflow::Result<String> {
        sqlx::query(
            "INSERT INTO sessions (session_id, client_type, state) VALUES (?, 'pi', 'working')",
        )
        .bind(&self.session_id)
        .execute(&self.pool)
        .await
        .expect("create workflow session");
        SqliteRuntimeBindingRepository::new(self.pool.clone())
            .upsert_binding(RuntimeBindingUpsertRecord {
                session_id: self.session_id.clone(),
                runtime_kind: "pi_tui".to_string(),
                runtime_instance_id: Some(self.runtime_instance_id.clone()),
                binding_state: "confirmed".to_string(),
                runtime_handle: None,
                start_command: None,
                launch_cwd: Some("/workspace/project".to_string()),
                internal_event_url: None,
                started_at: None,
                last_seen_at: None,
                restart_count: 0,
                tmux_socket_path: Some("/tmp/fake-tmux.sock".to_string()),
                tmux_pane_id: Some("%42".to_string()),
                process_fingerprint: None,
                capabilities: "{}".to_string(),
                diagnostics: "{}".to_string(),
                adapter_details: "{}".to_string(),
            })
            .await
            .expect("bind workflow runtime");
        Ok(self.session_id.clone())
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
            if session_id == "session_submit" && runtime_instance_id == "rtinst_submit" {
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
    pool: sqlx::SqlitePool,
    sender: broadcast::Sender<DomainEvent>,
}

impl TestAgentEvents {
    fn new(pool: sqlx::SqlitePool) -> Self {
        let (sender, _) = broadcast::channel(16);
        Self { pool, sender }
    }

    async fn publish(&self, event: DomainEvent) {
        let turn_id = event.turn_id.clone().or_else(|| {
            event
                .event_type
                .is_turn_event()
                .then(|| "turn_root".to_string())
        });
        sqlx::query(
            r#"INSERT OR IGNORE INTO events
               (event_id, session_id, turn_id, source, client_type, event_type, occurred_at, payload)
               VALUES (?, ?, ?, ?, ?, ?, '2026-07-31T00:00:00Z', ?)"#,
        )
        .bind(&event.event_id)
        .bind(&event.session_id)
        .bind(turn_id)
        .bind(event.source.to_string())
        .bind(&event.client_type)
        .bind(event.event_type.to_string())
        .bind(event.payload.to_string())
        .execute(&self.pool)
        .await
        .expect("persist Agent fact before publishing wake-up hint");
        self.sender.send(event).expect("workflow event subscriber");
    }
}

impl AgentEventSubscriber for TestAgentEvents {
    fn subscribe(&self) -> broadcast::Receiver<DomainEvent> {
        self.sender.subscribe()
    }
}

fn spawn_coordinator(
    pool: sqlx::SqlitePool,
    sessions: BoundSessionCreator,
    exits: RecordingExitRequester,
    events: TestAgentEvents,
    pontia_home: std::path::PathBuf,
) -> tokio::task::JoinHandle<()> {
    let (shutdown_tx, shutdown) = tokio::sync::watch::channel(false);
    tokio::spawn(async move {
        let _keepalive = shutdown_tx;
        WorkflowCoordinator::with_services(pool, sessions, exits, events, pontia_home)
            .run(shutdown)
            .await;
    })
}

async fn test_pool(path: &Path) -> sqlx::SqlitePool {
    let database_url = format!("sqlite://{}", path.display());
    let pool = connect_sqlite(&database_url).await.expect("connect");
    run_migrations(&pool).await.expect("migrate");
    pool
}

async fn seed_running_single_node(
    repository: &SqliteWorkflowRepository,
    workflow_id: &str,
    node_id: &str,
) {
    repository
        .create_workflow(CreateWorkflowRecord {
            workflow_id: workflow_id.to_string(),
            title: "Single node workflow".to_string(),
            cwd: "/workspace/project".to_string(),
            state: "pending".to_string(),
        })
        .await
        .expect("create workflow");
    repository
        .create_node(CreateWorkflowNodeRecord {
            node_id: node_id.to_string(),
            workflow_id: workflow_id.to_string(),
            parent_node_id: None,
            phase: "Test Phase".to_string(),
            title: "Writer".to_string(),
            instructions: "Write the result.".to_string(),
            inputs: "[]".to_string(),
            output: "result.md".to_string(),
            execution_profile_id: None,
            execution_profile_version: None,
        })
        .await
        .expect("create node");
}

fn event(
    event_id: &str,
    session_id: &str,
    event_type: EventType,
    runtime_instance_id: &str,
) -> DomainEvent {
    let source = if event_type.is_turn_event() {
        EventSource::AgentAdapter
    } else {
        EventSource::AgentClient
    };
    DomainEvent::new(
        event_id.to_string(),
        session_id.to_string(),
        None,
        source,
        "pi".to_string(),
        event_type,
        json!({ "runtime_instance_id": runtime_instance_id }),
    )
}

async fn wait_for_exit_request(exits: &RecordingExitRequester) {
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        loop {
            if !exits
                .requests
                .lock()
                .expect("exit requests lock")
                .is_empty()
            {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        }
    })
    .await
    .expect("workflow did not request graceful exit");
}

async fn wait_for_state(repository: &SqliteWorkflowRepository, expected: &str) {
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        loop {
            let state = repository
                .get_workflow("wf_submit")
                .await
                .expect("load workflow")
                .expect("workflow exists")
                .state;
            if state == expected {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("workflow did not reach {expected}"));
}

#[tokio::test]
async fn submission_writes_handoff_and_waits_for_confirmed_session_exit_before_completion() {
    let temp = tempfile::tempdir().expect("tempdir");
    let pool = test_pool(&temp.path().join("submit.db")).await;
    let repository = SqliteWorkflowRepository::new(pool.clone());
    seed_running_single_node(&repository, "wf_submit", "node_submit").await;
    let sessions = BoundSessionCreator {
        pool: pool.clone(),
        session_id: "session_submit".to_string(),
        runtime_instance_id: "rtinst_submit".to_string(),
    };
    let exits = RecordingExitRequester::default();
    let events = TestAgentEvents::new(pool.clone());
    let pontia_home = temp.path().join("pontia-home");
    let _coordinator = spawn_coordinator(
        pool.clone(),
        sessions.clone(),
        exits.clone(),
        events.clone(),
        pontia_home.clone(),
    );
    let scheduler = WorkflowScheduler::with_services(pool, sessions, exits.clone(), pontia_home);
    scheduler.start("wf_submit").await.expect("start workflow");

    scheduler
        .submit(SubmitWorkflowNodeRequest {
            session_id: "session_submit".to_string(),
            runtime_instance_id: "rtinst_submit".to_string(),
            output: "result.md".to_string(),
            content: "Complete UTF-8 handoff: 完成\n".to_string(),
        })
        .await
        .expect("submit output");

    assert_eq!(
        std::fs::read_to_string(
            temp.path()
                .join("pontia-home/workflows/wf_submit/handoff/result.md")
        )
        .expect("read handoff"),
        "Complete UTF-8 handoff: 完成\n"
    );
    assert!(
        exits
            .requests
            .lock()
            .expect("exit requests lock")
            .is_empty(),
        "submission must not exit pi before turn.completed timeline enrichment"
    );
    let node = repository
        .get_node("node_submit")
        .await
        .expect("load node")
        .expect("node exists");
    assert!(node.submitted_at.is_some());
    assert_eq!(
        repository
            .get_workflow("wf_submit")
            .await
            .expect("load workflow")
            .expect("workflow exists")
            .state,
        "running"
    );

    events
        .publish(event(
            "evt_turn_completed",
            "session_submit",
            EventType::TurnCompleted,
            "rtinst_submit",
        ))
        .await;
    wait_for_exit_request(&exits).await;
    assert_eq!(
        exits
            .requests
            .lock()
            .expect("exit requests lock")
            .as_slice(),
        &[("session_submit".to_string(), "rtinst_submit".to_string())]
    );
    let node = repository
        .get_node("node_submit")
        .await
        .expect("load node after Turn completion")
        .expect("node exists");
    assert_eq!(
        node.submitted_runtime_instance_id.as_deref(),
        Some("rtinst_submit")
    );
    assert!(node.exit_request_started_at.is_some());
    events
        .publish(event(
            "evt_turn_completed_duplicate",
            "session_submit",
            EventType::TurnCompleted,
            "rtinst_replacement",
        ))
        .await;
    events
        .publish(event(
            "evt_turn_failed_after_submission",
            "session_submit",
            EventType::TurnFailed,
            "rtinst_submit",
        ))
        .await;
    events
        .publish(event(
            "evt_turn_interrupted_after_submission",
            "session_submit",
            EventType::TurnInterrupted,
            "rtinst_submit",
        ))
        .await;
    tokio::task::yield_now().await;
    assert_eq!(
        exits
            .requests
            .lock()
            .expect("exit requests lock")
            .as_slice(),
        &[("session_submit".to_string(), "rtinst_submit".to_string())],
        "terminal duplicates must not request exit again or target a replacement runtime"
    );
    assert_eq!(
        repository
            .get_workflow("wf_submit")
            .await
            .expect("load workflow")
            .expect("workflow exists")
            .state,
        "running"
    );

    events
        .publish(event(
            "evt_other_session_exited",
            "session_other",
            EventType::SessionExited,
            "rtinst_other",
        ))
        .await;
    tokio::task::yield_now().await;
    assert_eq!(
        repository
            .get_workflow("wf_submit")
            .await
            .expect("load workflow")
            .expect("workflow exists")
            .state,
        "running"
    );

    let mut unauthorized_exit = event(
        "evt_unauthorized_session_exited",
        "session_submit",
        EventType::SessionExited,
        "rtinst_submit",
    );
    unauthorized_exit.source = EventSource::ExternalApi;
    events.publish(unauthorized_exit).await;
    tokio::task::yield_now().await;
    assert_eq!(
        repository
            .get_workflow("wf_submit")
            .await
            .expect("load workflow")
            .expect("workflow exists")
            .state,
        "running"
    );

    let mut runtime_observed_exit = event(
        "evt_session_exited",
        "session_submit",
        EventType::SessionExited,
        "rtinst_submit",
    );
    runtime_observed_exit.source = EventSource::RuntimeManager;
    events.publish(runtime_observed_exit).await;
    wait_for_state(&repository, "completed").await;

    let workflow = repository
        .get_workflow("wf_submit")
        .await
        .expect("load workflow")
        .expect("workflow exists");
    assert!(workflow.completed_at.is_some());
    let workflow_events = repository
        .list_events("wf_submit")
        .await
        .expect("list workflow events");
    assert_eq!(
        workflow_events
            .iter()
            .map(|event| event.event_type.as_str())
            .collect::<Vec<_>>(),
        vec!["workflow.started", "workflow.completed"]
    );
}

#[tokio::test]
async fn submission_rejects_wrong_session_runtime_and_output_without_writing_or_exiting() {
    let temp = tempfile::tempdir().expect("tempdir");
    let pool = test_pool(&temp.path().join("reject.db")).await;
    let repository = SqliteWorkflowRepository::new(pool.clone());
    seed_running_single_node(&repository, "wf_submit", "node_submit").await;
    let sessions = BoundSessionCreator {
        pool: pool.clone(),
        session_id: "session_submit".to_string(),
        runtime_instance_id: "rtinst_submit".to_string(),
    };
    let exits = RecordingExitRequester::default();
    let scheduler = WorkflowScheduler::with_services(
        pool,
        sessions,
        exits.clone(),
        temp.path().join("pontia-home"),
    );
    scheduler.start("wf_submit").await.expect("start workflow");

    for (session_id, runtime_instance_id, output, expected) in [
        (
            "session_other",
            "rtinst_submit",
            "result.md",
            "session_other",
        ),
        (
            "session_submit",
            "rtinst_stale",
            "result.md",
            "current runtime",
        ),
        (
            "session_submit",
            "rtinst_submit",
            "other.md",
            "declared output",
        ),
    ] {
        let error = scheduler
            .submit(SubmitWorkflowNodeRequest {
                session_id: session_id.to_string(),
                runtime_instance_id: runtime_instance_id.to_string(),
                output: output.to_string(),
                content: "must not be written".to_string(),
            })
            .await
            .expect_err("invalid submission must fail");
        assert!(error.to_string().contains(expected), "{error}");
    }

    assert!(
        !temp
            .path()
            .join("pontia-home/workflows/wf_submit/handoff/result.md")
            .exists()
    );
    assert!(
        exits
            .requests
            .lock()
            .expect("exit requests lock")
            .is_empty()
    );
}

#[tokio::test]
async fn submission_rejects_a_node_whose_workflow_is_not_running() {
    let temp = tempfile::tempdir().expect("tempdir");
    let pool = test_pool(&temp.path().join("not-running.db")).await;
    let repository = SqliteWorkflowRepository::new(pool.clone());
    seed_running_single_node(&repository, "wf_submit", "node_submit").await;
    let sessions = BoundSessionCreator {
        pool: pool.clone(),
        session_id: "session_submit".to_string(),
        runtime_instance_id: "rtinst_submit".to_string(),
    };
    sessions
        .create_session(CreateSessionRequest {
            client_type: "pi".to_string(),
            title: None,
            workspace: None,
            workspace_id: None,
            handle: None,
            role: None,
            description: None,
            execution_profile_id: None,
            execution_profile_version: None,
            metadata: json!({}),
            initial_task: None,
            runtime_environment: Default::default(),
        })
        .await
        .expect("create bound session");
    repository
        .bind_node_session("node_submit", "session_submit")
        .await
        .expect("bind node session");
    let exits = RecordingExitRequester::default();
    let scheduler = WorkflowScheduler::with_services(
        pool,
        sessions,
        exits.clone(),
        temp.path().join("pontia-home"),
    );

    let error = scheduler
        .submit(SubmitWorkflowNodeRequest {
            session_id: "session_submit".to_string(),
            runtime_instance_id: "rtinst_submit".to_string(),
            output: "result.md".to_string(),
            content: "must not be written".to_string(),
        })
        .await
        .expect_err("pending workflow submission must fail");

    assert!(error.to_string().contains("must be running"), "{error}");
    assert!(
        exits
            .requests
            .lock()
            .expect("exit requests lock")
            .is_empty()
    );
}
