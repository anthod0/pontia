use pontia_core::domain::EventType;
use pontia_storage_sqlite::repositories::workflows::SqliteWorkflowRepository;
use pontia_workflow::WorkflowScheduler;

use crate::{
    fixture::{seed_linear_workflow, test_pool, wait_for_state},
    test_doubles::{RecordingExitRequester, SequencedSessionCreator, TestAgentEvents},
};

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
               VALUES (?, 'session_root', 'turn_root', 'agent_adapter', 'pi', ?,
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
