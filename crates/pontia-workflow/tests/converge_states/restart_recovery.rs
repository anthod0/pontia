use pontia_workflow::WorkflowRuntime;
use pontia_workflow::repository::WorkflowState;

use crate::{
    fixture::{seed_linear_workflow, test_pool, wait_for_state},
    test_doubles::{RecordingExitRequester, SequencedSessionCreator},
};

#[tokio::test]
async fn startup_recovers_a_running_workflow_from_persisted_session_exit() {
    let temp = tempfile::tempdir().expect("tempdir");
    let pool = test_pool(&temp.path().join("restart-recovery.db")).await;
    let repository = WorkflowState::new(
        pool.clone(),
        &pontia_agent_control::AgentControl::builder(pool.clone(), temp.path().into()).build(),
    )
    .await
    .unwrap();
    seed_linear_workflow(&repository, "wf_restart", "[]", false).await;
    repository
        .start_workflow("wf_restart", "evt_workflow_started")
        .await
        .expect("start workflow");
    repository
        .bind_node_session("wf_restart_root", "session_root")
        .await
        .expect("bind existing Session");
    repository
        .record_node_submission(
            "wf_restart_root",
            "runtime_session_root",
            "evt_submit_restart",
        )
        .await
        .expect("record existing submission");
    sqlx::query(
        r#"INSERT INTO events
           (event_id, session_id, source, client_type, event_type, occurred_at, payload)
           VALUES ('evt_session_exited', 'session_root', 'agent_client', 'pi',
                   'session.exited', '2026-07-31T00:00:00Z',
                   '{"runtime_id":"runtime_session_root"}')"#,
    )
    .execute(&pool)
    .await
    .expect("persist confirmed Session exit");

    let (shutdown, shutdown_rx) = tokio::sync::watch::channel(false);
    let coordinator = WorkflowRuntime::with_services(
        repository.clone(),
        &pontia_agent_control::AgentControl::builder(pool.clone(), temp.path().join("pontia-home"))
            .clients(crate::test_doubles::clients())
            .build(),
        SequencedSessionCreator::new([]),
        RecordingExitRequester::default(),
        temp.path().join("pontia-home"),
    );
    let task = tokio::spawn(coordinator.run(shutdown_rx));

    wait_for_state(&repository, "wf_restart", "completed").await;
    shutdown.send(true).expect("signal shutdown");
    task.await.expect("coordinator task").unwrap();

    let events = repository
        .list_events("wf_restart")
        .await
        .expect("list Workflow events");
    assert_eq!(
        events
            .iter()
            .map(|event| event.event_type.as_str())
            .collect::<Vec<_>>(),
        vec![
            "workflow.started",
            "workflow.node_submitted",
            "workflow.completed",
        ]
    );
}

#[tokio::test]
async fn repeated_reconciliation_activates_a_downstream_node_once() {
    let temp = tempfile::tempdir().expect("tempdir");
    let pool = test_pool(&temp.path().join("idempotent-reconciliation.db")).await;
    let repository = WorkflowState::new(
        pool.clone(),
        &pontia_agent_control::AgentControl::builder(pool.clone(), temp.path().into()).build(),
    )
    .await
    .unwrap();
    seed_linear_workflow(&repository, "wf_repeat", "[]", true).await;
    repository
        .start_workflow("wf_repeat", "evt_workflow_started")
        .await
        .expect("start workflow");
    repository
        .bind_node_session("wf_repeat_root", "session_root")
        .await
        .expect("bind existing Session");
    repository
        .record_node_submission(
            "wf_repeat_root",
            "runtime_session_root",
            "evt_submit_repeat",
        )
        .await
        .expect("record existing submission");
    sqlx::query(
        r#"INSERT INTO events
           (event_id, session_id, source, client_type, event_type, occurred_at, payload)
           VALUES ('evt_session_exited', 'session_root', 'agent_client', 'pi',
                   'session.exited', '2026-07-31T00:00:00Z', '{}')"#,
    )
    .execute(&pool)
    .await
    .expect("persist confirmed Session exit");
    let pontia_home = temp.path().join("pontia-home");
    let handoff_dir = pontia_home.join("workflows/wf_repeat/handoff");
    std::fs::create_dir_all(&handoff_dir).expect("create isolated Handoff directory");
    std::fs::write(handoff_dir.join("root.md"), "root output").expect("write downstream input");

    let sessions = SequencedSessionCreator::new([Some("session_child")]);
    let coordinator = WorkflowRuntime::with_services(
        repository.clone(),
        &pontia_agent_control::AgentControl::builder(pool.clone(), temp.path().join("pontia-home"))
            .clients(crate::test_doubles::clients())
            .build(),
        sessions.clone(),
        RecordingExitRequester::default(),
        pontia_home,
    );

    coordinator
        .reconcile("wf_repeat")
        .await
        .expect("first reconciliation");
    coordinator
        .reconcile("wf_repeat")
        .await
        .expect("repeated reconciliation");

    assert_eq!(
        sessions
            .requests
            .lock()
            .expect("Session requests lock")
            .len(),
        1
    );
    assert_eq!(
        repository
            .get_node("wf_repeat_child")
            .await
            .expect("load child")
            .expect("child exists")
            .session_id
            .as_deref(),
        Some("session_child")
    );
}

#[tokio::test]
async fn restart_recovery_does_not_treat_a_pause_interruption_as_failure() {
    let temp = tempfile::tempdir().expect("tempdir");
    let pool = test_pool(&temp.path().join("paused-interruption.db")).await;
    let repository = WorkflowState::new(
        pool.clone(),
        &pontia_agent_control::AgentControl::builder(pool.clone(), temp.path().into()).build(),
    )
    .await
    .unwrap();
    seed_linear_workflow(&repository, "wf_paused_restart", "[]", false).await;
    repository
        .start_workflow("wf_paused_restart", "evt_workflow_started")
        .await
        .expect("start workflow");
    repository
        .bind_node_session("wf_paused_restart_root", "session_root")
        .await
        .expect("bind existing Session");
    repository
        .pause_workflow("wf_paused_restart", "evt_workflow_paused")
        .await
        .expect("pause workflow");
    sqlx::query(
        r#"INSERT INTO events
           (event_id, session_id, turn_id, source, client_type, event_type, occurred_at, payload)
           VALUES ('evt_turn_interrupted', 'session_root', 'turn_root', 'agent_adapter', 'pi',
                   'turn.interrupted', '2026-07-31T00:00:00Z',
                   '{"runtime_id":"runtime_session_root"}')"#,
    )
    .execute(&pool)
    .await
    .expect("persist pause interruption");
    repository
        .resume_workflow("wf_paused_restart", "evt_workflow_resumed")
        .await
        .expect("resume workflow");

    let coordinator = WorkflowRuntime::with_services(
        repository.clone(),
        &pontia_agent_control::AgentControl::builder(pool.clone(), temp.path().join("pontia-home"))
            .clients(crate::test_doubles::clients())
            .build(),
        SequencedSessionCreator::new([]),
        RecordingExitRequester::default(),
        temp.path().join("pontia-home"),
    );
    coordinator
        .reconcile("wf_paused_restart")
        .await
        .expect("reconcile resumed Workflow");

    assert_eq!(
        repository
            .get_workflow("wf_paused_restart")
            .await
            .expect("load Workflow")
            .expect("Workflow exists")
            .state,
        "running"
    );
}

#[tokio::test]
async fn committed_session_exit_wakes_the_idle_workflow_runtime() {
    let temp = tempfile::tempdir().expect("tempdir");
    let pool = test_pool(&temp.path().join("missed-notification.db")).await;
    let app = pontia_agent_control::AgentControl::builder(pool.clone(), temp.path().into())
        .clients(crate::test_doubles::clients())
        .build();
    let repository = WorkflowState::new(pool.clone(), &app).await.unwrap();
    seed_linear_workflow(&repository, "wf_missed", "[]", false).await;
    repository
        .start_workflow("wf_missed", "evt_workflow_started")
        .await
        .expect("start workflow");
    repository
        .bind_node_session("wf_missed_root", "session_root")
        .await
        .expect("bind existing Session");
    repository
        .record_node_submission(
            "wf_missed_root",
            "runtime_session_root",
            "evt_submit_missed",
        )
        .await
        .expect("record existing submission");

    let (shutdown, shutdown_rx) = tokio::sync::watch::channel(false);
    let coordinator = WorkflowRuntime::with_services(
        repository.clone(),
        &app,
        SequencedSessionCreator::new([]),
        RecordingExitRequester::default(),
        temp.path().join("pontia-home"),
    );
    let task = tokio::spawn(coordinator.run(shutdown_rx));
    use pontia_core::domain::{EventSource, EventType, ReportedEvent};
    let ingest = app.event_ingest_service();
    ingest
        .ingest_reported_event(ReportedEvent::new(
            "created".into(),
            "session_root".into(),
            None,
            EventSource::ExternalApi,
            "pi".into(),
            EventType::SessionCreated,
            serde_json::json!({}),
        ))
        .await
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let projected: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM workflow_agent_events WHERE event_id='created')",
            )
            .fetch_one(&pool)
            .await
            .unwrap();
            if projected {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    ingest
        .ingest_reported_event(ReportedEvent::new(
            "exited".into(),
            "session_root".into(),
            None,
            EventSource::AgentClient,
            "pi".into(),
            EventType::SessionExited,
            serde_json::json!({"runtime_id":"runtime_session_root"}),
        ))
        .await
        .unwrap();
    wait_for_state(&repository, "wf_missed", "completed").await;
    shutdown.send(true).expect("signal shutdown");
    task.await.expect("coordinator task").unwrap();
}
