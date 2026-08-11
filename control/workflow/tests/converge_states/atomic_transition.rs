use pontia_storage_sqlite::repositories::workflows::SqliteWorkflowRepository;

use crate::fixture::{seed_linear_workflow, test_pool};

#[tokio::test]
async fn submission_and_terminal_transition_have_one_atomic_winner() {
    let temp = tempfile::tempdir().expect("tempdir");
    let pool = test_pool(&temp.path().join("atomic-race.db")).await;
    let repository = SqliteWorkflowRepository::new(pool);

    for index in 0..20 {
        let workflow_id = format!("wf_atomic_{index}");
        let node_id = format!("{workflow_id}_root");
        seed_linear_workflow(&repository, &workflow_id, "[]", false).await;
        repository
            .start_workflow(&workflow_id, &format!("evt_started_{index}"))
            .await
            .expect("start workflow record");

        let submission_repository = repository.clone();
        let submission_node_id = node_id.clone();
        let terminal_repository = repository.clone();
        let terminal_workflow_id = workflow_id.clone();
        let terminal_node_id = node_id.clone();
        let (submission, terminal) = tokio::join!(
            async move {
                submission_repository
                    .record_node_submission(&submission_node_id)
                    .await
            },
            async move {
                terminal_repository
                    .idle_unsubmitted_workflow_node(
                        &terminal_workflow_id,
                        &terminal_node_id,
                        &format!("evt_idle_{index}"),
                    )
                    .await
            }
        );

        assert_ne!(submission.is_ok(), terminal.is_ok());
        let workflow = repository
            .get_workflow(&workflow_id)
            .await
            .expect("load workflow")
            .expect("workflow exists");
        let node = repository
            .get_node(&node_id)
            .await
            .expect("load node")
            .expect("node exists");
        if submission.is_ok() {
            assert_eq!(workflow.state, "running");
            assert!(node.submitted_at.is_some());
        } else {
            assert_eq!(workflow.state, "idle");
            assert!(node.submitted_at.is_none());
        }
    }
}
