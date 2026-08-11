use pontia_core::domain::EventType;
use pontia_storage_sqlite::repositories::workflows::SqliteWorkflowRepository;
use pontia_workflow::{SubmitWorkflowNodeRequest, WorkflowScheduler};

use crate::{
    fixture::{assert_transition, seed_linear_workflow, test_pool, wait_for_state},
    test_doubles::{RecordingExitRequester, SequencedSessionCreator, TestAgentEvents},
};

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
