use axum::http::StatusCode;
use pontia_application::{AppState, EventIngestService};
use pontia_core::domain::EventType;
use serde_json::json;

use super::fixture::{bind_runtime, create_session, post_event, post_json, test_state};

async fn unbound_reporting_workflow_state() -> AppState {
    use pontia_storage_sqlite::repositories::workflows::{
        CreateWorkflowNodeRecord, CreateWorkflowRecord, SqliteWorkflowRepository,
    };
    let state = test_state().await;
    create_session(&state, "sess_reporting", "pi").await;
    bind_runtime(&state, "sess_reporting", "rtinst_reporting").await;
    let repository = SqliteWorkflowRepository::new(state.db());
    repository
        .create_definition(
            CreateWorkflowRecord {
                workflow_id: "wf_reporting".into(),
                title: "Reporting failure".into(),
                cwd: "/tmp".into(),
                state: "pending".into(),
            },
            ["node_reporting", "node_downstream"]
                .into_iter()
                .enumerate()
                .map(|(index, node_id)| CreateWorkflowNodeRecord {
                    workflow_id: "wf_reporting".into(),
                    node_id: node_id.into(),
                    parent_node_id: (index == 1).then(|| "node_reporting".into()),
                    phase: "implement".into(),
                    title: "implement".into(),
                    instructions: "task".into(),
                    inputs: "[]".into(),
                    output: format!("{node_id}.md"),
                    execution_profile_id: None,
                    execution_profile_version: None,
                })
                .collect(),
        )
        .await
        .unwrap();
    repository
        .start_workflow("wf_reporting", "evt_start_workflow")
        .await
        .unwrap();
    state
}

async fn reporting_workflow_state() -> AppState {
    let state = unbound_reporting_workflow_state().await;
    pontia_storage_sqlite::repositories::workflows::SqliteWorkflowRepository::new(state.db())
        .bind_node_session("node_reporting", "sess_reporting")
        .await
        .unwrap();
    state
}

async fn reconcile_reporting_workflow(state: &AppState) {
    struct NoSessions;
    impl pontia_workflow::SessionCreator for NoSessions {
        async fn create_session(
            &self,
            _: pontia_application::CreateSessionRequest,
        ) -> pontia_workflow::Result<String> {
            panic!("reporting failure must not activate a downstream Session");
        }
    }
    let root = tempfile::tempdir().unwrap();
    pontia_workflow::WorkflowCoordinator::new(
        state.event_ingest_service(),
        NoSessions,
        state.agent_events(),
        root.path().to_path_buf(),
    )
    .reconcile("wf_reporting")
    .await
    .unwrap();
}

#[tokio::test]
async fn turn_start_reporting_failure_fails_workflow_without_fabricating_turn_failure() {
    use pontia_storage_sqlite::repositories::workflows::SqliteWorkflowRepository;
    use pontia_workflow::WorkflowQueryService;
    let state = reporting_workflow_state().await;
    let repository = SqliteWorkflowRepository::new(state.db());
    let path = "/internal/v1/sessions/sess_reporting/turn-start-failure";

    let (status, _) = post_json(
        state.clone(),
        path,
        json!({
            "runtime_instance_id": "rtinst_stale", "reason": "event_rejected"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(
        repository
            .get_workflow("wf_reporting")
            .await
            .unwrap()
            .unwrap()
            .state,
        "running"
    );

    let (status, body) = post_event(
        state.clone(),
        json!({
            "session_id": "sess_reporting",
            "type": "turn.started",
            "data": {
                "runtime_instance_id": "rtinst_reporting",
                "topology_context": { "oversized": "x".repeat(70_000) }
            }
        }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body:?}");
    assert!(
        repository
            .record_node_submission("node_reporting", "rtinst_reporting", "evt_invalid_submit")
            .await
            .is_err()
    );
    reconcile_reporting_workflow(&state).await;
    assert_eq!(
        repository
            .get_workflow("wf_reporting")
            .await
            .unwrap()
            .unwrap()
            .state,
        "failed"
    );

    for _ in 0..2 {
        let (status, body) = post_json(
            state.clone(),
            path,
            json!({
                "runtime_instance_id": "rtinst_reporting", "reason": "event_rejected"
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body:?}");
    }
    let workflow = WorkflowQueryService::new(state.db())
        .get_workflow("wf_reporting")
        .await
        .unwrap();
    let value = serde_json::to_value(workflow).unwrap();
    assert_eq!(value["state"], "failed");
    assert_eq!(value["nodes"][0]["status"], "failed");
    assert!(
        value["failure_message"]
            .as_str()
            .unwrap()
            .contains("turn.started reporting failed")
    );
    let timeline = repository.list_events("wf_reporting").await.unwrap();
    assert_eq!(
        timeline
            .iter()
            .filter(|event| event.event_type == "workflow.failed")
            .count(),
        1
    );
    let events = EventIngestService::new(state.db())
        .list_events("sess_reporting")
        .await
        .unwrap();
    assert!(
        !events
            .iter()
            .any(|event| event.event_type == EventType::TurnFailed)
    );
}

#[tokio::test]
async fn turn_start_failure_notification_fails_workflow_when_original_event_never_arrives() {
    use pontia_workflow::WorkflowQueryService;
    for reason in ["transport_failed", "missing_turn_id"] {
        let state = reporting_workflow_state().await;
        let (status, body) = post_json(
            state.clone(),
            "/internal/v1/sessions/sess_reporting/turn-start-failure",
            json!({ "runtime_instance_id": "rtinst_reporting", "reason": reason }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body:?}");
        reconcile_reporting_workflow(&state).await;
        let workflow = WorkflowQueryService::new(state.db())
            .get_workflow("wf_reporting")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(workflow.state, "failed");
        assert!(workflow.failure_message.unwrap().contains(reason));
    }
}

#[tokio::test]
async fn reporting_failure_before_node_binding_is_recovered_from_persisted_events() {
    use pontia_storage_sqlite::repositories::workflows::SqliteWorkflowRepository;
    let state = unbound_reporting_workflow_state().await;
    let (status, body) = post_json(
        state.clone(),
        "/internal/v1/sessions/sess_reporting/turn-start-failure",
        json!({ "runtime_instance_id": "rtinst_reporting", "reason": "transport_failed" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");
    let repository = SqliteWorkflowRepository::new(state.db());
    repository
        .bind_node_session("node_reporting", "sess_reporting")
        .await
        .unwrap();
    // A fresh coordinator has never seen the HTTP request or its notification.
    reconcile_reporting_workflow(&state).await;
    assert_eq!(
        repository
            .get_workflow("wf_reporting")
            .await
            .unwrap()
            .unwrap()
            .state,
        "failed"
    );
}

#[tokio::test]
async fn lost_started_response_administratively_abandons_the_committed_turn() {
    let state = reporting_workflow_state().await;
    let (status, started) = post_event(
        state.clone(),
        json!({
            "session_id": "sess_reporting", "type": "turn.started",
            "data": { "runtime_instance_id": "rtinst_reporting", "input_summary": "task" }
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = post_json(
        state.clone(),
        "/internal/v1/sessions/sess_reporting/turn-start-failure",
        json!({"runtime_instance_id": "rtinst_reporting", "reason": "transport_failed"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let turn = EventIngestService::new(state.db())
        .get_turn(started["turn_id"].as_str().unwrap())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(turn.state, pontia_core::domain::TurnState::Abandoned);
    assert_eq!(
        turn.metadata["terminal_provenance"]["reason"],
        "session_error_without_terminal_fact"
    );
}

#[tokio::test]
async fn reporting_failure_also_fails_a_submitted_node_waiting_for_exit() {
    use pontia_storage_sqlite::repositories::workflows::SqliteWorkflowRepository;
    let state = reporting_workflow_state().await;
    let repository = SqliteWorkflowRepository::new(state.db());
    repository
        .record_node_submission("node_reporting", "rtinst_reporting", "evt_submitted")
        .await
        .unwrap();
    let (status, _) = post_json(
        state.clone(),
        "/internal/v1/sessions/sess_reporting/turn-start-failure",
        json!({"runtime_instance_id": "rtinst_reporting", "reason": "transport_failed"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    reconcile_reporting_workflow(&state).await;
    let workflow = pontia_workflow::WorkflowQueryService::new(state.db())
        .get_workflow("wf_reporting")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(workflow.state, "failed");
    assert_eq!(
        workflow.nodes[0].status,
        pontia_workflow::WorkflowAgentStatus::Failed
    );
    assert_eq!(workflow.current_node_id.as_deref(), Some("node_reporting"));
    assert_eq!(
        workflow.nodes[1].status,
        pontia_workflow::WorkflowAgentStatus::Pending
    );
    assert!(workflow.nodes[1].session_id.is_none());
}

#[tokio::test]
async fn reporting_failure_notifications_persist_only_one_error_per_runtime() {
    let state = reporting_workflow_state().await;
    for _ in 0..3 {
        let (status, _) = post_json(
            state.clone(),
            "/internal/v1/sessions/sess_reporting/turn-start-failure",
            json!({"runtime_instance_id": "rtinst_reporting", "reason": "transport_failed"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }
    let events = EventIngestService::new(state.db())
        .list_events("sess_reporting")
        .await
        .unwrap();
    assert_eq!(
        events
            .iter()
            .filter(|event| event.event_type == EventType::SessionError)
            .count(),
        1
    );
}

#[tokio::test]
async fn reporting_failure_from_an_old_runtime_cannot_fail_its_replacement() {
    let state = reporting_workflow_state().await;
    let (status, _) = post_json(
        state.clone(),
        "/internal/v1/sessions/sess_reporting/turn-start-failure",
        json!({"runtime_instance_id": "rtinst_reporting", "reason": "transport_failed"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    bind_runtime(&state, "sess_reporting", "rtinst_replacement").await;
    reconcile_reporting_workflow(&state).await;
    let workflow = pontia_workflow::WorkflowQueryService::new(state.db())
        .get_workflow("wf_reporting")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(workflow.state, "running");
}
