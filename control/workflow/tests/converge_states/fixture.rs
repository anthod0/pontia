use std::{path::Path, time::Duration};

use pontia_storage_sqlite::{
    connect_sqlite,
    repositories::workflows::{
        CreateWorkflowNodeRecord, CreateWorkflowRecord, SqliteWorkflowRepository,
    },
    run_migrations,
};

pub(super) async fn test_pool(path: &Path) -> sqlx::SqlitePool {
    let database_url = format!("sqlite://{}", path.display());
    let pool = connect_sqlite(&database_url).await.expect("connect");
    run_migrations(&pool).await.expect("migrate");
    pool
}

pub(super) async fn seed_linear_workflow(
    repository: &SqliteWorkflowRepository,
    workflow_id: &str,
    root_inputs: &str,
    with_child: bool,
) {
    repository
        .create_workflow(CreateWorkflowRecord {
            workflow_id: workflow_id.to_string(),
            title: "Convergence workflow".to_string(),
            cwd: "/workspace/project".to_string(),
            state: "pending".to_string(),
        })
        .await
        .expect("create workflow");
    repository
        .create_node(CreateWorkflowNodeRecord {
            node_id: format!("{workflow_id}_root"),
            workflow_id: workflow_id.to_string(),
            parent_node_id: None,
            title: "Root".to_string(),
            instructions: "Produce the root output.".to_string(),
            inputs: root_inputs.to_string(),
            output: "root.md".to_string(),
            execution_profile_id: None,
            execution_profile_version: None,
        })
        .await
        .expect("create root node");
    if with_child {
        repository
            .create_node(CreateWorkflowNodeRecord {
                node_id: format!("{workflow_id}_child"),
                workflow_id: workflow_id.to_string(),
                parent_node_id: Some(format!("{workflow_id}_root")),
                title: "Child".to_string(),
                instructions: "Produce the child output.".to_string(),
                inputs: "[\"root.md\"]".to_string(),
                output: "child.md".to_string(),
                execution_profile_id: None,
                execution_profile_version: None,
            })
            .await
            .expect("create child node");
    }
}

pub(super) async fn wait_for_state(
    repository: &SqliteWorkflowRepository,
    workflow_id: &str,
    expected: &str,
) {
    for _ in 0..200 {
        let workflow = repository
            .get_workflow(workflow_id)
            .await
            .expect("load workflow")
            .expect("workflow exists");
        if workflow.state == expected {
            return;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("workflow {workflow_id} did not reach {expected}");
}

pub(super) async fn assert_transition(
    repository: &SqliteWorkflowRepository,
    workflow_id: &str,
    expected_state: &str,
    expected_event: &str,
) -> Option<String> {
    let workflow = repository
        .get_workflow(workflow_id)
        .await
        .expect("load workflow")
        .expect("workflow exists");
    assert_eq!(workflow.state, expected_state);
    let events = repository
        .list_events(workflow_id)
        .await
        .expect("list workflow events");
    assert_eq!(events.len(), 2);
    assert_eq!(events[1].event_type, expected_event);
    workflow.failure_message
}
