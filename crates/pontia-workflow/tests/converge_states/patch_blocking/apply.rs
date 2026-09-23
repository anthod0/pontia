use pontia_storage_sqlite::repositories::workflows::SqliteWorkflowRepository;
use pontia_workflow::{
    ApplyWorkflowPatch, RequestWorkflowPatch, WorkflowCoordinator, WorkflowPatchService,
};

use crate::{
    fixture::test_pool,
    test_doubles::{RecordingExitRequester, TestAgentEvents},
};

use super::fixture::{
    PersistingSessionCreator, insert_fact, seed_requester, write_patch_file, write_problem_report,
};

#[tokio::test]
async fn changed_apply_revises_the_graph_and_queues_one_continuation_without_planner_exit() {
    let temp = tempfile::tempdir().expect("tempdir");
    let pontia_home = temp.path().join("pontia-home");
    std::fs::create_dir(&pontia_home).unwrap();
    let pool = test_pool(&temp.path().join("patch-apply.db")).await;
    let repository = SqliteWorkflowRepository::new(pool.clone());
    seed_requester(&pool, &repository, &pontia_home, "wf_apply", true).await;
    write_problem_report(&pontia_home, "wf_apply", "Replace the remaining work.");
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
        "evt_apply_interrupted",
        "sess_requester",
        "turn_requester",
        "turn.interrupted",
        "runtime_requester",
    )
    .await;
    let exits = RecordingExitRequester::default();
    let coordinator = WorkflowCoordinator::with_services(
        pontia_application::EventIngestService::new(pool.clone()),
        PersistingSessionCreator::new(pool.clone()),
        exits.clone(),
        TestAgentEvents::new(pool.clone()),
        pontia_home.clone(),
    );
    coordinator.reconcile("wf_apply").await.unwrap();

    let workflow_file = pontia_home.join("workflows/wf_apply/workflow.toml");
    std::fs::write(&workflow_file, "not valid = [").unwrap();
    coordinator.reconcile("wf_apply").await.unwrap();
    assert_eq!(
        std::fs::read_to_string(&workflow_file).unwrap(),
        "not valid = [",
        "an active Re-planner draft must not be repaired"
    );
    write_patch_file(
        &pontia_home,
        "wf_apply",
        &patch_id,
        "decision.md",
        "Candidate needs correction.",
    );
    let invalid = WorkflowPatchService::new(pool.clone(), pontia_home.clone())
        .apply_patch(ApplyWorkflowPatch {
            session_id: "sess_replanner".into(),
            runtime_instance_id: "runtime_replanner".into(),
        })
        .await;
    assert!(invalid.is_err());
    assert_eq!(
        repository
            .get_workflow("wf_apply")
            .await
            .unwrap()
            .unwrap()
            .state,
        "replanning"
    );
    assert_eq!(
        repository
            .get_patch(&patch_id)
            .await
            .unwrap()
            .unwrap()
            .state,
        "planning"
    );

    std::fs::write(
        &workflow_file,
        r#"workflow_id = "wf_apply"
revision = 1
title = "Convergence workflow"
cwd = "/workspace/project"

[[nodes]]
id = "wf_apply_root"
type = "agent"
phase = "Test Phase"
title = "Root"
instructions = "Produce the root output."
inputs = []
output = "root.md"

[[nodes]]
type = "agent"
phase = "Replanned"
title = "Replacement"
instructions = "Complete the replacement."
inputs = ["root.md"]
output = "replacement.md"
"#,
    )
    .unwrap();
    let service = WorkflowPatchService::new(pool.clone(), pontia_home.clone());
    write_patch_file(
        &pontia_home,
        "wf_apply",
        &patch_id,
        "decision.md",
        &format!("{} END-OF-DOCUMENT", "replacement rationale ".repeat(40)),
    );
    let applied = service
        .apply_patch(ApplyWorkflowPatch {
            session_id: "sess_replanner".into(),
            runtime_instance_id: "runtime_replanner".into(),
        })
        .await
        .expect("apply changed Patch");
    assert_eq!(applied.patch_id, patch_id);
    assert_eq!(applied.outcome, "applied");
    assert_eq!(applied.revision, 2);

    let workflow = repository.get_workflow("wf_apply").await.unwrap().unwrap();
    assert_eq!(workflow.state, "running");
    assert_eq!(workflow.current_revision, 2);
    let history = repository.list_node_history("wf_apply").await.unwrap();
    let retired = history
        .iter()
        .find(|node| node.node_id == "wf_apply_child")
        .unwrap();
    assert_eq!(retired.retired_revision, Some(2));
    let replacement = history
        .iter()
        .find(|node| node.introduced_revision == 2)
        .expect("replacement Node");
    assert_eq!(replacement.parent_node_id.as_deref(), Some("wf_apply_root"));
    assert_eq!(replacement.title, "Replacement");
    let event = repository
        .list_events("wf_apply")
        .await
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(event.event_type, "workflow.patch_applied");
    assert!(!event.payload.contains("END-OF-DOCUMENT"));

    let duplicate = service
        .apply_patch(ApplyWorkflowPatch {
            session_id: "sess_replanner".into(),
            runtime_instance_id: "runtime_replanner".into(),
        })
        .await;
    assert!(duplicate.is_err());
    assert_eq!(
        repository
            .list_node_history("wf_apply")
            .await
            .unwrap()
            .len(),
        3
    );

    coordinator.reconcile("wf_apply").await.unwrap();
    coordinator.reconcile("wf_apply").await.unwrap();
    let patch = repository.get_patch(&patch_id).await.unwrap().unwrap();
    assert!(patch.continuation_queued_at.is_some());
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM inbox_messages WHERE session_id = 'sess_requester'"
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        1
    );
    assert_eq!(
        repository
            .get_workflow("wf_apply")
            .await
            .unwrap()
            .unwrap()
            .state,
        "running",
        "the Patch-owned interruption must not fail resumed execution"
    );
    assert!(exits.requests.lock().unwrap().is_empty());

    std::fs::remove_file(&workflow_file).unwrap();
    coordinator.reconcile("wf_apply").await.unwrap();
    let repaired = std::fs::read_to_string(&workflow_file).unwrap();
    assert!(repaired.contains("revision = 2"));
    assert!(repaired.contains("title = \"Replacement\""));

    sqlx::query("UPDATE turns SET state = 'interrupted' WHERE turn_id = 'turn_requester'")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO turns (turn_id, session_id, state, topology_status) VALUES ('turn_requester_second', 'sess_requester', 'running', 'root')")
        .execute(&pool).await.unwrap();
    sqlx::query("UPDATE sessions SET current_turn_id = 'turn_requester_second', state = 'busy' WHERE session_id = 'sess_requester'")
        .execute(&pool).await.unwrap();
    insert_fact(
        &pool,
        "evt_requester_second_started",
        "sess_requester",
        "turn_requester_second",
        "turn.started",
        "runtime_requester",
    )
    .await;
    write_problem_report(
        &pontia_home,
        "wf_apply",
        "Refine the replacement once more.",
    );
    let second_patch_id = WorkflowPatchService::new(pool.clone(), pontia_home.clone())
        .request_patch(RequestWorkflowPatch {
            session_id: "sess_requester".into(),
            runtime_instance_id: "runtime_requester".into(),
        })
        .await
        .unwrap()
        .patch_id;
    insert_fact(
        &pool,
        "evt_apply_interrupted_second",
        "sess_requester",
        "turn_requester_second",
        "turn.interrupted",
        "runtime_requester",
    )
    .await;
    let second_coordinator = WorkflowCoordinator::with_services(
        pontia_application::EventIngestService::new(pool.clone()),
        PersistingSessionCreator::with_identity(
            pool.clone(),
            "sess_replanner_second",
            "turn_replanner_second",
            "runtime_replanner_second",
        ),
        RecordingExitRequester::default(),
        TestAgentEvents::new(pool.clone()),
        pontia_home.clone(),
    );
    second_coordinator.reconcile("wf_apply").await.unwrap();
    std::fs::write(
        &workflow_file,
        r#"workflow_id = "wf_apply"
revision = 2
title = "Convergence workflow"
cwd = "/workspace/project"

[[nodes]]
id = "wf_apply_root"
type = "agent"
phase = "Test Phase"
title = "Root"
instructions = "Produce the root output."
inputs = []
output = "root.md"

[[nodes]]
type = "agent"
phase = "Final plan"
title = "Final replacement"
instructions = "Complete the final replacement."
inputs = ["root.md"]
output = "final.md"
"#,
    )
    .unwrap();
    write_patch_file(
        &pontia_home,
        "wf_apply",
        &second_patch_id,
        "decision.md",
        "Use the final replacement.",
    );
    let second_outcome = WorkflowPatchService::new(pool.clone(), pontia_home.clone())
        .apply_patch(ApplyWorkflowPatch {
            session_id: "sess_replanner_second".into(),
            runtime_instance_id: "runtime_replanner_second".into(),
        })
        .await
        .unwrap();
    assert_eq!(second_outcome.revision, 3);
    let final_history = repository.list_node_history("wf_apply").await.unwrap();
    assert_eq!(final_history.len(), 4);
    assert_eq!(
        final_history
            .iter()
            .find(|node| node.node_id == replacement.node_id)
            .unwrap()
            .retired_revision,
        Some(3)
    );
    assert_eq!(
        repository
            .get_patch(&second_patch_id)
            .await
            .unwrap()
            .unwrap()
            .state,
        "applied"
    );
}

#[tokio::test]
async fn unchanged_apply_rejects_without_advancing_the_revision() {
    let temp = tempfile::tempdir().unwrap();
    let pontia_home = temp.path().join("pontia-home");
    std::fs::create_dir(&pontia_home).unwrap();
    let pool = test_pool(&temp.path().join("patch-reject.db")).await;
    let repository = SqliteWorkflowRepository::new(pool.clone());
    seed_requester(&pool, &repository, &pontia_home, "wf_reject", false).await;
    write_problem_report(
        &pontia_home,
        "wf_reject",
        "Check whether a change is needed.",
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
        "evt_reject_interrupted",
        "sess_requester",
        "turn_requester",
        "turn.interrupted",
        "runtime_requester",
    )
    .await;
    WorkflowCoordinator::with_services(
        pontia_application::EventIngestService::new(pool.clone()),
        PersistingSessionCreator::new(pool.clone()),
        RecordingExitRequester::default(),
        TestAgentEvents::new(pool.clone()),
        pontia_home.clone(),
    )
    .reconcile("wf_reject")
    .await
    .unwrap();

    write_patch_file(
        &pontia_home,
        "wf_reject",
        &patch_id,
        "decision.md",
        "The accepted plan remains valid.",
    );
    let outcome = WorkflowPatchService::new(pool.clone(), pontia_home.clone())
        .apply_patch(ApplyWorkflowPatch {
            session_id: "sess_replanner".into(),
            runtime_instance_id: "runtime_replanner".into(),
        })
        .await
        .expect("reject unchanged Patch");
    assert_eq!(outcome.outcome, "rejected");
    assert_eq!(outcome.revision, 1);
    assert_eq!(
        repository
            .get_workflow("wf_reject")
            .await
            .unwrap()
            .unwrap()
            .current_revision,
        1
    );
    assert_eq!(
        repository
            .get_patch(&patch_id)
            .await
            .unwrap()
            .unwrap()
            .state,
        "rejected"
    );
    assert_eq!(
        repository
            .list_events("wf_reject")
            .await
            .unwrap()
            .pop()
            .unwrap()
            .event_type,
        "workflow.patch_rejected"
    );
}
