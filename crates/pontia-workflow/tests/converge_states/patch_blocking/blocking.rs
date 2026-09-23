use pontia_storage_sqlite::repositories::workflows::SqliteWorkflowRepository;
use pontia_workflow::{
    BlockWorkflowPatch, RequestWorkflowPatch, WorkflowCoordinator, WorkflowPatchService,
};

use crate::{
    fixture::test_pool,
    test_doubles::{RecordingExitRequester, TestAgentEvents},
};

use super::fixture::{
    PersistingSessionCreator, insert_fact, seed_requester, write_patch_file, write_problem_report,
};

#[tokio::test]
async fn confirmed_interruption_creates_one_real_replanner_and_explicit_block_is_fenced() {
    let temp = tempfile::tempdir().expect("tempdir");
    let pontia_home = temp.path().join("pontia-home");
    std::fs::create_dir(&pontia_home).expect("Pontia home");
    let pool = test_pool(&temp.path().join("patch-blocking.db")).await;
    let repository = SqliteWorkflowRepository::new(pool.clone());
    seed_requester(&pool, &repository, &pontia_home, "wf_block", false).await;

    write_problem_report(
        &pontia_home,
        "wf_block",
        "The remaining plan cannot be completed safely.",
    );
    let patch_id = WorkflowPatchService::new(pool.clone(), pontia_home.clone())
        .request_patch(RequestWorkflowPatch {
            session_id: "sess_requester".into(),
            runtime_instance_id: "runtime_requester".into(),
        })
        .await
        .expect("request Patch")
        .patch_id;
    insert_fact(
        &pool,
        "evt_requester_interrupted",
        "sess_requester",
        "turn_requester",
        "turn.interrupted",
        "runtime_requester",
    )
    .await;

    let creator = PersistingSessionCreator::new(pool.clone());
    let exits = RecordingExitRequester::default();
    let coordinator = WorkflowCoordinator::with_services(
        &pontia_application::AppState::builder(pool.clone(), pontia_home.clone())
            .clients(crate::test_doubles::clients())
            .build(),
        creator.clone(),
        exits.clone(),
        TestAgentEvents::new(pool.clone()),
        pontia_home.clone(),
    );
    coordinator
        .reconcile("wf_block")
        .await
        .expect("create Re-planner");
    coordinator
        .reconcile("wf_block")
        .await
        .expect("idempotent reconcile");

    {
        let requests = creator.requests.lock().expect("requests");
        assert_eq!(requests.len(), 1);
        let request = &requests[0];
        assert_eq!(request.client_type, "pi");
        assert_eq!(request.role.as_deref(), Some("workflow_replanner"));
        assert_eq!(request.workspace.as_deref(), Some("/workspace/project"));
        assert_eq!(request.metadata["workflow_id"], "wf_block");
        assert_eq!(request.metadata["workflow_patch_id"], patch_id);
        assert!(request.metadata["workflow_replanner_creation_token"].is_string());
        assert_eq!(
            request.runtime_environment["PONTIA_WORKFLOW_ID"],
            "wf_block"
        );
        assert_eq!(
            request.runtime_environment["PONTIA_WORKFLOW_PATCH_ID"],
            patch_id
        );
        let initial_task = request.initial_task.as_ref().expect("initial task");
        assert!(
            initial_task
                .input
                .contains("The remaining plan cannot be completed safely.")
        );
        assert!(
            initial_task.input.contains(
                &pontia_home
                    .join("workflows/wf_block/workflow.toml")
                    .display()
                    .to_string()
            )
        );
        assert!(
            initial_task.input.contains(
                &pontia_home
                    .join(format!("workflows/wf_block/patches/{patch_id}/decision.md"))
                    .display()
                    .to_string()
            )
        );
        assert!(
            initial_task.input.contains(
                &pontia_home
                    .join(format!("workflows/wf_block/patches/{patch_id}/reason.md"))
                    .display()
                    .to_string()
            )
        );
    }

    let patch = repository.get_patch(&patch_id).await.unwrap().unwrap();
    assert_eq!(patch.state, "planning");
    assert_eq!(
        patch.replanner_session_id.as_deref(),
        Some("sess_replanner")
    );
    assert_eq!(
        patch.replanner_runtime_instance_id.as_deref(),
        Some("runtime_replanner")
    );
    assert!(
        repository
            .list_nodes("wf_block")
            .await
            .unwrap()
            .iter()
            .all(|node| node.session_id.as_deref() != Some("sess_replanner"))
    );

    let workflow_file = pontia_home.join("workflows/wf_block/workflow.toml");
    let accepted = std::fs::read_to_string(&workflow_file).unwrap();
    std::fs::write(&workflow_file, "unaccepted draft").unwrap();
    write_patch_file(
        &pontia_home,
        "wf_block",
        &patch_id,
        "reason.md",
        "No executable continuation exists.",
    );
    let outcome = WorkflowPatchService::new(pool.clone(), pontia_home.clone())
        .block_patch(BlockWorkflowPatch {
            session_id: "sess_replanner".into(),
            runtime_instance_id: "runtime_replanner".into(),
        })
        .await
        .expect("block Patch");
    assert_eq!(outcome.patch_id, patch_id);
    assert_eq!(std::fs::read_to_string(&workflow_file).unwrap(), accepted);
    let blocked = repository.get_patch(&patch_id).await.unwrap().unwrap();
    let workflow_dir = pontia_home.join("workflows/wf_block");
    assert_eq!(
        std::fs::read_to_string(workflow_dir.join(blocked.reason_document_ref.as_deref().unwrap()))
            .unwrap(),
        "No executable continuation exists."
    );
    assert_eq!(
        std::fs::read_to_string(workflow_dir.join(blocked.blocked_draft_ref.as_deref().unwrap()))
            .unwrap(),
        "unaccepted draft"
    );

    assert_eq!(blocked.state, "blocked");
    assert_eq!(blocked.replanner_turn_id.as_deref(), Some("turn_replanner"));
    assert_eq!(
        repository
            .get_workflow("wf_block")
            .await
            .unwrap()
            .unwrap()
            .state,
        "blocked"
    );
    assert_eq!(
        repository
            .list_events("wf_block")
            .await
            .unwrap()
            .last()
            .unwrap()
            .event_type,
        "workflow.patch_blocked"
    );

    let stale = WorkflowPatchService::new(pool.clone(), pontia_home.clone())
        .block_patch(BlockWorkflowPatch {
            session_id: "sess_replanner".into(),
            runtime_instance_id: "runtime_replanner".into(),
        })
        .await;
    assert!(stale.is_err());
    assert_eq!(
        std::fs::read_to_string(workflow_dir.join(blocked.reason_document_ref.as_deref().unwrap()))
            .unwrap(),
        "No executable continuation exists."
    );

    insert_fact(
        &pool,
        "evt_replanner_completed",
        "sess_replanner",
        "turn_replanner",
        "turn.completed",
        "runtime_replanner",
    )
    .await;
    coordinator
        .reconcile("wf_block")
        .await
        .expect("request graceful exit");
    assert_eq!(
        exits.requests.lock().expect("exit requests").as_slice(),
        &[("sess_replanner".into(), "runtime_replanner".into())]
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM events WHERE session_id = 'sess_replanner' AND event_type = 'session.exited'")
            .fetch_one(&pool).await.unwrap(),
        0
    );
}

#[tokio::test]
async fn requester_terminal_fact_implicitly_blocks_and_preserves_the_accepted_outcome() {
    let temp = tempfile::tempdir().unwrap();
    let pontia_home = temp.path().join("pontia-home");
    std::fs::create_dir(&pontia_home).unwrap();
    let pool = test_pool(&temp.path().join("requester-terminal.db")).await;
    let repository = SqliteWorkflowRepository::new(pool.clone());
    seed_requester(
        &pool,
        &repository,
        &pontia_home,
        "wf_requester_terminal",
        false,
    )
    .await;
    write_problem_report(
        &pontia_home,
        "wf_requester_terminal",
        "Re-plan before continuing",
    );
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
        "evt_requester_failed",
        "sess_requester",
        "turn_requester",
        "turn.failed",
        "runtime_requester",
    )
    .await;

    let coordinator = WorkflowCoordinator::with_services(
        &pontia_application::AppState::builder(pool.clone(), pontia_home.clone())
            .clients(crate::test_doubles::clients())
            .build(),
        PersistingSessionCreator::new(pool.clone()),
        RecordingExitRequester::default(),
        TestAgentEvents::new(pool.clone()),
        pontia_home.clone(),
    );
    coordinator
        .reconcile("wf_requester_terminal")
        .await
        .unwrap();

    let patch = repository.get_patch(&patch_id).await.unwrap().unwrap();
    assert_eq!(patch.state, "blocked");
    assert!(patch.reason_document_ref.is_some());
    assert_eq!(
        repository
            .get_workflow("wf_requester_terminal")
            .await
            .unwrap()
            .unwrap()
            .state,
        "blocked"
    );
    let events = repository
        .list_events("wf_requester_terminal")
        .await
        .unwrap();
    assert_eq!(
        events
            .iter()
            .filter(|event| event.event_type == "workflow.patch_blocked")
            .count(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM events WHERE event_type IN ('turn.completed', 'session.exited')"
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        0,
        "the coordinator must not fabricate Agent lifecycle facts"
    );
}

#[tokio::test]
async fn unresolved_replanner_terminal_blocks_once_restores_definition_and_late_facts_do_not_change_it()
 {
    let temp = tempfile::tempdir().unwrap();
    let pontia_home = temp.path().join("pontia-home");
    std::fs::create_dir(&pontia_home).unwrap();
    let pool = test_pool(&temp.path().join("replanner-terminal.db")).await;
    let repository = SqliteWorkflowRepository::new(pool.clone());
    seed_requester(
        &pool,
        &repository,
        &pontia_home,
        "wf_planner_terminal",
        false,
    )
    .await;
    write_problem_report(&pontia_home, "wf_planner_terminal", "Re-plan");
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
        "evt_requester_interrupted_terminal_case",
        "sess_requester",
        "turn_requester",
        "turn.interrupted",
        "runtime_requester",
    )
    .await;
    let exits = RecordingExitRequester::default();
    let coordinator = WorkflowCoordinator::with_services(
        &pontia_application::AppState::builder(pool.clone(), pontia_home.clone())
            .clients(crate::test_doubles::clients())
            .build(),
        PersistingSessionCreator::new(pool.clone()),
        exits.clone(),
        TestAgentEvents::new(pool.clone()),
        pontia_home.clone(),
    );
    coordinator.reconcile("wf_planner_terminal").await.unwrap();
    let workflow_file = pontia_home.join("workflows/wf_planner_terminal/workflow.toml");
    let accepted = std::fs::read_to_string(&workflow_file).unwrap();
    std::fs::write(&workflow_file, "unfinished draft").unwrap();
    insert_fact(
        &pool,
        "evt_replanner_failed_unresolved",
        "sess_replanner",
        "turn_replanner",
        "turn.failed",
        "runtime_replanner",
    )
    .await;

    coordinator.reconcile("wf_planner_terminal").await.unwrap();
    coordinator.reconcile("wf_planner_terminal").await.unwrap();
    let blocked = repository.get_patch(&patch_id).await.unwrap().unwrap();
    assert_eq!(blocked.state, "blocked");
    assert_eq!(blocked.replanner_turn_id.as_deref(), Some("turn_replanner"));
    assert!(blocked.blocked_draft_ref.is_some());
    assert_eq!(std::fs::read_to_string(&workflow_file).unwrap(), accepted);
    assert_eq!(
        repository
            .list_events("wf_planner_terminal")
            .await
            .unwrap()
            .iter()
            .filter(|event| event.event_type == "workflow.patch_blocked")
            .count(),
        1
    );
    assert_eq!(exits.requests.lock().unwrap().len(), 1);

    insert_fact(
        &pool,
        "evt_replanner_completed_late",
        "sess_replanner",
        "turn_replanner",
        "turn.completed",
        "runtime_replanner",
    )
    .await;
    coordinator.reconcile("wf_planner_terminal").await.unwrap();
    assert_eq!(
        repository.get_patch(&patch_id).await.unwrap().unwrap(),
        blocked
    );
}
