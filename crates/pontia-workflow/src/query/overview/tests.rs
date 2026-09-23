use pontia_storage_sqlite::models::workflows::{WorkflowNodeRow, WorkflowRow};

use super::{WorkflowAgentStatus, derive_status};

fn workflow(state: &str) -> WorkflowRow {
    WorkflowRow {
        workflow_id: "wf_test".to_string(),
        title: "Test".to_string(),
        cwd: "/tmp".to_string(),
        state: state.to_string(),
        current_revision: 1,
        failure_message: None,
        created_at: "2026-08-14T00:00:00Z".to_string(),
        updated_at: "2026-08-14T00:01:00Z".to_string(),
        started_at: Some("2026-08-14T00:00:00Z".to_string()),
        completed_at: None,
    }
}

fn node(session_id: Option<&str>, submitted: bool) -> WorkflowNodeRow {
    WorkflowNodeRow {
        node_id: "node_test".to_string(),
        workflow_id: "wf_test".to_string(),
        parent_node_id: None,
        node_type: "agent".to_string(),
        phase: "Test".to_string(),
        title: "Test".to_string(),
        instructions: String::new(),
        inputs: "[]".to_string(),
        output: "out.md".to_string(),
        execution_profile_id: None,
        execution_profile_version: None,
        introduced_revision: 1,
        retired_revision: None,
        session_id: session_id.map(str::to_string),
        submitted_at: submitted.then(|| "2026-08-14T00:00:30Z".to_string()),
        submitted_runtime_instance_id: submitted.then(|| "rtinst_test".to_string()),
        exit_request_started_at: None,
        created_at: "2026-08-14T00:00:00Z".to_string(),
    }
}

#[test]
fn derives_agent_status_from_workflow_facts_and_session_projection() {
    assert_eq!(
        derive_status(&workflow("running"), &node(None, false), None, false, true),
        WorkflowAgentStatus::Pending
    );
    assert_eq!(
        derive_status(
            &workflow("running"),
            &node(Some("s"), false),
            Some("starting"),
            false,
            true
        ),
        WorkflowAgentStatus::Starting
    );
    assert_eq!(
        derive_status(
            &workflow("running"),
            &node(Some("s"), false),
            Some("busy"),
            false,
            true
        ),
        WorkflowAgentStatus::Running
    );
    assert_eq!(
        derive_status(
            &workflow("paused"),
            &node(Some("s"), false),
            Some("interrupted"),
            false,
            true
        ),
        WorkflowAgentStatus::Paused
    );
    assert_eq!(
        derive_status(
            &workflow("paused"),
            &node(Some("s"), true),
            Some("interrupted"),
            false,
            true
        ),
        WorkflowAgentStatus::Paused
    );
    assert_eq!(
        derive_status(
            &workflow("idle"),
            &node(Some("s"), false),
            Some("idle"),
            false,
            true
        ),
        WorkflowAgentStatus::Idle
    );
    assert_eq!(
        derive_status(
            &workflow("running"),
            &node(Some("s"), true),
            Some("idle"),
            false,
            true
        ),
        WorkflowAgentStatus::Exiting
    );
    assert_eq!(
        derive_status(
            &workflow("running"),
            &node(Some("s"), true),
            Some("exited"),
            false,
            true
        ),
        WorkflowAgentStatus::Submitted
    );
    assert_eq!(
        derive_status(&workflow("failed"), &node(None, false), None, true, true),
        WorkflowAgentStatus::Failed
    );
    assert_eq!(
        derive_status(
            &workflow("running"),
            &node(Some("missing"), false),
            None,
            false,
            true
        ),
        WorkflowAgentStatus::Unknown
    );
}

#[test]
fn missing_bound_session_remains_unknown_even_at_failure_location() {
    assert_eq!(
        derive_status(
            &workflow("failed"),
            &node(Some("missing"), false),
            None,
            true,
            true
        ),
        WorkflowAgentStatus::Unknown,
    );
}
