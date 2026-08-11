use std::{sync::Arc, time::Duration};

use pontia_core::domain::EventType;
use pontia_storage_sqlite::repositories::workflows::SqliteWorkflowRepository;
use pontia_workflow::{SubmitWorkflowNodeRequest, WorkflowScheduler};

use crate::{
    fixture::{assert_transition, seed_linear_workflow, test_pool, wait_for_state},
    test_doubles::{RecordingExitRequester, SequencedSessionCreator, TestAgentEvents},
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
