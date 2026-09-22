use pontia_core::domain::EventType;
use pontia_storage_sqlite::repositories::workflows::SqliteWorkflowRepository;
use pontia_workflow::WorkflowScheduler;

use crate::{
    fixture::{assert_transition, seed_linear_workflow, test_pool, wait_for_state},
    test_doubles::{
        RecordingExitRequester, SequencedSessionCreator, TestAgentEvents, spawn_coordinator,
    },
};

#[tokio::test]
async fn unsubmitted_completed_turn_enters_idle_and_keeps_the_current_session() {
    let temp = tempfile::tempdir().expect("tempdir");
    let pool = test_pool(&temp.path().join("idle.db")).await;
    let repository = SqliteWorkflowRepository::new(pool.clone());
    seed_linear_workflow(&repository, "wf_idle", "[]", true).await;
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
    scheduler.start("wf_idle").await.expect("start workflow");

    events
        .publish("session_root", EventType::TurnCompleted)
        .await;
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
