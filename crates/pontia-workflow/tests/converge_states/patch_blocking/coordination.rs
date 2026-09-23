use pontia_storage_sqlite::repositories::workflows::SqliteWorkflowRepository;
use pontia_workflow::{RequestWorkflowPatch, WorkflowCoordinator, WorkflowPatchService};
use serde_json::json;

use crate::{
    fixture::test_pool,
    test_doubles::{RecordingExitRequester, TestAgentEvents},
};

use super::fixture::{
    PersistingSessionCreator, insert_fact, seed_replanner_session, seed_requester,
    write_problem_report,
};

#[tokio::test]
async fn crash_gap_recovers_the_session_with_the_persisted_creation_token() {
    let temp = tempfile::tempdir().expect("tempdir");
    let pontia_home = temp.path().join("pontia-home");
    std::fs::create_dir(&pontia_home).unwrap();
    let pool = test_pool(&temp.path().join("patch-replanner-recovery.db")).await;
    let repository = SqliteWorkflowRepository::new(pool.clone());
    seed_requester(&pool, &repository, &pontia_home, "wf_recover", false).await;
    write_problem_report(&pontia_home, "wf_recover", "Recover planning");
    let patch_id = WorkflowPatchService::new(pool.clone(), pontia_home.clone())
        .request_patch(RequestWorkflowPatch {
            session_id: "sess_requester".into(),
            runtime_instance_id: "runtime_requester".into(),
        })
        .await
        .unwrap()
        .patch_id;
    insert_fact(
        &pool,
        "evt_recover_interrupted",
        "sess_requester",
        "turn_requester",
        "turn.interrupted",
        "runtime_requester",
    )
    .await;
    let token = repository
        .get_patch(&patch_id)
        .await
        .unwrap()
        .unwrap()
        .replanner_creation_token;
    seed_replanner_session(
        &pool,
        "sess_recovered_replanner",
        "turn_recovered_replanner",
        "runtime_recovered_replanner",
        &json!({ "workflow_replanner_creation_token": token }),
    )
    .await;

    let creator = PersistingSessionCreator::new(pool.clone());
    let coordinator_one = WorkflowCoordinator::with_services(
        &pontia_application::AppState::builder(pool.clone(), pontia_home.clone())
            .clients(crate::test_doubles::clients())
            .build(),
        creator.clone(),
        RecordingExitRequester::default(),
        TestAgentEvents::new(pool.clone()),
        pontia_home.clone(),
    );
    let coordinator_two = WorkflowCoordinator::with_services(
        &pontia_application::AppState::builder(pool.clone(), pontia_home.clone())
            .clients(crate::test_doubles::clients())
            .build(),
        creator.clone(),
        RecordingExitRequester::default(),
        TestAgentEvents::new(pool.clone()),
        pontia_home,
    );
    let (first, second) = tokio::join!(
        coordinator_one.reconcile("wf_recover"),
        coordinator_two.reconcile("wf_recover")
    );
    first.unwrap();
    second.unwrap();

    assert!(creator.requests.lock().unwrap().is_empty());
    let patch = repository.get_patch(&patch_id).await.unwrap().unwrap();
    assert_eq!(patch.state, "planning");
    assert_eq!(
        patch.replanner_session_id.as_deref(),
        Some("sess_recovered_replanner")
    );
    assert_eq!(
        repository
            .list_events("wf_recover")
            .await
            .unwrap()
            .iter()
            .filter(|event| event.event_type == "workflow.replanner_started")
            .count(),
        1
    );
}

#[tokio::test]
async fn simultaneous_patch_requests_accept_exactly_one_active_patch() {
    let temp = tempfile::tempdir().unwrap();
    let pontia_home = temp.path().join("pontia-home");
    std::fs::create_dir(&pontia_home).unwrap();
    let pool = test_pool(&temp.path().join("concurrent-patch-request.db")).await;
    let repository = SqliteWorkflowRepository::new(pool.clone());
    seed_requester(
        &pool,
        &repository,
        &pontia_home,
        "wf_concurrent_patch",
        false,
    )
    .await;
    write_problem_report(&pontia_home, "wf_concurrent_patch", "concurrent request");
    let first = WorkflowPatchService::new(pool.clone(), pontia_home.clone());
    let second = WorkflowPatchService::new(pool.clone(), pontia_home);
    let (first, second) = tokio::join!(
        first.request_patch(RequestWorkflowPatch {
            session_id: "sess_requester".into(),
            runtime_instance_id: "runtime_requester".into(),
        }),
        second.request_patch(RequestWorkflowPatch {
            session_id: "sess_requester".into(),
            runtime_instance_id: "runtime_requester".into(),
        })
    );
    assert_eq!(usize::from(first.is_ok()) + usize::from(second.is_ok()), 1);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM workflow_patches WHERE workflow_id = 'wf_concurrent_patch' AND state IN ('requested', 'planning')"
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        1
    );
}
