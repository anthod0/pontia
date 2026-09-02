use pontia_core::domain::EventType;
use pontia_storage_sqlite::repositories::workflows::SqliteWorkflowRepository;
use pontia_workflow::{SubmitWorkflowNodeRequest, WorkflowScheduler};

use crate::{
    fixture::{assert_transition, seed_linear_workflow, test_pool, wait_for_state},
    test_doubles::{
        RecordingExitRequester, SequencedSessionCreator, TestAgentEvents, spawn_coordinator,
    },
};

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
        let events = TestAgentEvents::new(pool.clone());
        let pontia_home = temp.path().join("pontia-home");
        let _coordinator = spawn_coordinator(
            pool.clone(),
            sessions.clone(),
            exits.clone(),
            events.clone(),
            pontia_home.clone(),
        );
        let scheduler =
            WorkflowScheduler::with_services(pool, sessions.clone(), exits.clone(), pontia_home);
        scheduler.start("wf_failure").await.expect("start workflow");

        events.publish("session_root", event_type).await;
        events.publish("session_root", event_type).await;
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
async fn submission_binding_failure_fails_without_starting_a_child() {
    let temp = tempfile::tempdir().expect("tempdir");
    let pool = test_pool(&temp.path().join("missing-binding.db")).await;
    let repository = SqliteWorkflowRepository::new(pool.clone());
    seed_linear_workflow(&repository, "wf_missing_binding", "[]", true).await;
    let sessions = SequencedSessionCreator::new([Some("session_root"), Some("session_child")]);
    let exits = RecordingExitRequester::missing_runtime_binding();
    let scheduler = WorkflowScheduler::with_services(
        pool,
        sessions.clone(),
        exits.clone(),
        temp.path().join("pontia-home"),
    );
    scheduler
        .start("wf_missing_binding")
        .await
        .expect("start workflow");

    let error = scheduler
        .submit(SubmitWorkflowNodeRequest {
            session_id: "session_root".to_string(),
            runtime_instance_id: "runtime_session_root".to_string(),
            output: "root.md".to_string(),
            content: "root output".to_string(),
        })
        .await
        .expect_err("submission without a runtime binding must fail");

    let failure_message = assert_transition(
        &repository,
        "wf_missing_binding",
        "failed",
        "workflow.failed",
    )
    .await
    .expect("failure message");
    assert!(failure_message.contains("runtime binding"), "{error}");
    assert!(
        exits
            .requests
            .lock()
            .expect("exit requests lock")
            .is_empty()
    );
    assert_eq!(sessions.requests.lock().expect("requests lock").len(), 1);
}

#[tokio::test]
async fn deferred_exit_failure_fails_without_starting_a_child() {
    let temp = tempfile::tempdir().expect("tempdir");
    let pool = test_pool(&temp.path().join("exit-failure.db")).await;
    let repository = SqliteWorkflowRepository::new(pool.clone());
    seed_linear_workflow(&repository, "wf_exit_failure", "[]", true).await;
    let sessions = SequencedSessionCreator::new([Some("session_root"), Some("session_child")]);
    let exits = RecordingExitRequester::failing_request();
    let events = TestAgentEvents::new(pool.clone());
    let pontia_home = temp.path().join("pontia-home");
    let _coordinator = spawn_coordinator(
        pool.clone(),
        sessions.clone(),
        exits.clone(),
        events.clone(),
        pontia_home.clone(),
    );
    let scheduler =
        WorkflowScheduler::with_services(pool, sessions.clone(), exits.clone(), pontia_home);
    scheduler
        .start("wf_exit_failure")
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
        .expect("submission records output before Turn completion");

    events
        .publish("session_root", EventType::TurnCompleted)
        .await;
    wait_for_state(&repository, "wf_exit_failure", "failed").await;

    let failure_message =
        assert_transition(&repository, "wf_exit_failure", "failed", "workflow.failed")
            .await
            .expect("failure message");
    assert!(failure_message.contains("graceful exit"));
    assert_eq!(exits.requests.lock().expect("exit requests lock").len(), 1);
    assert_eq!(sessions.requests.lock().expect("requests lock").len(), 1);
    assert!(
        repository
            .get_node("wf_exit_failure_child")
            .await
            .expect("load child")
            .expect("child exists")
            .session_id
            .is_none()
    );
}
