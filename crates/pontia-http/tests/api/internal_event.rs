use crate::common::test_app::TestApp;
use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use pontia_application::{AppState, EventIngestService};
use pontia_core::{
    domain::{EventSource, EventType, ReportedEvent},
    ids::{new_event_id, new_turn_id},
};
use pontia_http as http;
use pontia_storage_sqlite::repositories::runtime_bindings::{
    RuntimeBindingUpsertRecord, SqliteRuntimeBindingRepository,
};
use serde_json::{Value, json};
use std::time::Duration;
use tower::ServiceExt;

async fn test_state() -> AppState {
    TestApp::builder()
        .database_name("internal_event.db")
        .external_api_token(None)
        .build_state()
        .await
}

async fn create_session(state: &AppState, session_id: &str, client_type: &str) {
    EventIngestService::new(state.db())
        .ingest_reported_event(ReportedEvent::new(
            new_event_id().to_string(),
            session_id.to_string(),
            None,
            EventSource::ExternalApi,
            client_type.to_string(),
            EventType::SessionCreated,
            json!({}),
        ))
        .await
        .expect("create session");
}

async fn bind_runtime(state: &AppState, session_id: &str, runtime_instance_id: &str) {
    SqliteRuntimeBindingRepository::new(state.db())
        .upsert_binding(RuntimeBindingUpsertRecord {
            session_id: session_id.to_string(),
            runtime_kind: "tmux".to_string(),
            runtime_instance_id: Some(runtime_instance_id.to_string()),
            binding_state: "confirmed".to_string(),
            runtime_handle: None,
            start_command: None,
            launch_cwd: Some("/tmp".to_string()),
            internal_event_url: None,
            started_at: None,
            last_seen_at: None,
            restart_count: 0,
            tmux_socket_path: None,
            tmux_pane_id: None,
            process_fingerprint: None,
            capabilities: "{}".to_string(),
            diagnostics: "{}".to_string(),
            adapter_details: "{}".to_string(),
        })
        .await
        .expect("bind runtime");
}

async fn post_event(state: AppState, body: Value) -> (StatusCode, Value) {
    post_json(state, "/internal/v1/events", body).await
}

async fn post_json(state: AppState, path: &str, body: Value) -> (StatusCode, Value) {
    let response = http::router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(path)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .expect("request"),
        )
        .await
        .expect("response");
    let status = response.status();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    (status, serde_json::from_slice(&bytes).expect("json body"))
}

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
        state.db(),
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

#[tokio::test]
async fn internal_event_api_rejects_pontia_owned_event_types() {
    let state = test_state().await;
    create_session(&state, "sess_owned_event", "generic").await;

    for fact_type in ["session.created", "turn.dispatch_failed", "turn.abandoned"] {
        let (status, body) = post_event(
            state.clone(),
            json!({
                "session_id": "sess_owned_event",
                "turn_id": "turn_owned_event",
                "type": fact_type,
                "data": {}
            }),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body:?}");
        assert!(
            body["error"]["message"]
                .as_str()
                .unwrap()
                .contains("owned by the Pontia control plane"),
            "{body:?}"
        );
    }
}

#[tokio::test]
async fn internal_event_api_rejects_timeline_boundary_as_an_unknown_field() {
    let state = test_state().await;

    let (status, body) = post_event(
        state,
        json!({
            "session_id": "sess_unknown_field",
            "type": "session.ready",
            "data": {},
            "timeline_boundary": null
        }),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "{body:?}");
    assert_eq!(body["error"]["code"], "invalid_request");
    assert!(
        body["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("unknown field `timeline_boundary`")),
        "{body:?}"
    );
}

#[tokio::test]
async fn internal_event_api_rejects_removed_timeline_item_events() {
    let state = test_state().await;

    let (status, body) = post_event(
        state,
        json!({
            "session_id": "sess_removed_timeline_event",
            "turn_id": "turn_removed_timeline_event",
            "type": "turn.timeline_item",
            "data": {}
        }),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "{body:?}");
    assert_eq!(body["error"]["code"], "invalid_request");
    assert!(
        body["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("unknown event type: turn.timeline_item")),
        "{body:?}"
    );
}

#[tokio::test]
async fn internal_event_api_normalizes_started_fact_into_a_domain_event() {
    let state = test_state().await;
    create_session(&state, "sess_normalized", "pi").await;
    bind_runtime(&state, "sess_normalized", "rtinst_normalized").await;

    let (status, body) = post_event(
        state.clone(),
        json!({
            "session_id": "sess_normalized",
            "type": "turn.started",
            "data": {
                "runtime_instance_id": "rtinst_normalized",
                "input_summary": "hello",
                "previous_leaf_id": null,
                "inbox_message_id": "msg_1"
            }
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body:?}");
    let event_id = body["event_id"].as_str().expect("event id");
    let turn_id = body["turn_id"].as_str().expect("turn id");
    assert!(event_id.starts_with("evt_"));
    assert!(turn_id.starts_with("turn_"));
    assert_eq!(
        turn_id[5..]
            .split('-')
            .nth(2)
            .and_then(|part| part.chars().next()),
        Some('7')
    );

    let events = EventIngestService::new(state.db())
        .list_events("sess_normalized")
        .await
        .expect("events");
    let started = events.last().expect("started event");
    assert_eq!(started.event_id, event_id);
    assert_eq!(started.turn_id.as_deref(), Some(turn_id));
    assert_eq!(started.source, EventSource::AgentAdapter);
    assert_eq!(started.client_type, "pi");
    assert_eq!(started.payload["input"]["summary"], "hello");
    assert_eq!(started.payload["metadata"]["inbox_message_id"], "msg_1");
}

#[tokio::test]
async fn internal_event_api_bounds_input_before_size_validation_and_event_storage() {
    let state = test_state().await;
    create_session(&state, "sess_long_input", "pi").await;
    bind_runtime(&state, "sess_long_input", "rtinst_long_input").await;
    let (status, body) = post_event(
        state.clone(),
        json!({
            "session_id": "sess_long_input",
            "type": "turn.started",
            "data": {
                "runtime_instance_id": "rtinst_long_input",
                "input_summary": "中😀".repeat(30_000)
            }
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");
    let events = EventIngestService::new(state.db())
        .list_events("sess_long_input")
        .await
        .expect("events");
    assert_eq!(
        events.last().unwrap().payload["input"]["summary"],
        "中😀".repeat(100)
    );
}

#[tokio::test]
async fn internal_event_api_broadcasts_a_started_fact_after_commit() {
    let state = test_state().await;
    create_session(&state, "sess_broadcast", "pi").await;
    bind_runtime(&state, "sess_broadcast", "rtinst_broadcast").await;
    let mut subscriber = state.agent_events().subscribe();

    let (status, body) = post_event(
        state.clone(),
        json!({
            "session_id": "sess_broadcast",
            "type": "turn.started",
            "data": { "runtime_instance_id": "rtinst_broadcast" }
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body:?}");
    let broadcast = tokio::time::timeout(Duration::from_secs(1), subscriber.recv())
        .await
        .expect("committed event should be broadcast")
        .expect("broker should stay open");
    assert_eq!(broadcast.event_id, body["event_id"]);
    assert_eq!(broadcast.turn_id.as_deref(), body["turn_id"].as_str());
    assert_eq!(broadcast.event_type, EventType::TurnStarted);
    let turn = EventIngestService::new(state.db())
        .get_turn(broadcast.turn_id.as_deref().expect("turn id"))
        .await
        .expect("turn query")
        .expect("projected turn");
    assert_eq!(turn.state.to_string(), "running");
}

#[tokio::test]
async fn internal_event_api_rejection_does_not_broadcast() {
    let state = test_state().await;
    create_session(&state, "sess_rejected_broadcast", "generic").await;
    let mut subscriber = state.agent_events().subscribe();

    let (status, body) = post_event(
        state.clone(),
        json!({
            "session_id": "sess_rejected_broadcast",
            "type": "turn.completed",
            "data": {}
        }),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "{body:?}");
    assert!(matches!(
        subscriber.try_recv(),
        Err(tokio::sync::broadcast::error::TryRecvError::Empty)
    ));
}

#[tokio::test]
async fn internal_event_ingestion_does_not_rebroadcast_a_duplicate_domain_event() {
    let state = test_state().await;
    create_session(&state, "sess_duplicate_broadcast", "generic").await;
    bind_runtime(
        &state,
        "sess_duplicate_broadcast",
        "rtinst_duplicate_broadcast",
    )
    .await;
    let mut subscriber = state.agent_events().subscribe();
    let service = EventIngestService::new(state.db()).with_agent_events(state.agent_events());
    let reported = ReportedEvent::new(
        "evt_duplicate_broadcast".to_string(),
        "sess_duplicate_broadcast".to_string(),
        None,
        EventSource::AgentClient,
        "generic".to_string(),
        EventType::SessionReady,
        json!({ "runtime_instance_id": "rtinst_duplicate_broadcast" }),
    );

    service
        .ingest_confirmed_event(reported.clone())
        .await
        .expect("first report");
    assert_eq!(
        subscriber
            .recv()
            .await
            .expect("first committed event")
            .event_id,
        "evt_duplicate_broadcast"
    );

    let duplicate = service
        .ingest_confirmed_event(reported)
        .await
        .expect("duplicate report");
    assert!(duplicate.duplicate);
    assert!(matches!(
        subscriber.try_recv(),
        Err(tokio::sync::broadcast::error::TryRecvError::Empty)
    ));
}

#[tokio::test]
async fn internal_event_api_rejects_supplied_unknown_turn_id_for_started_fact() {
    let state = test_state().await;
    create_session(&state, "sess_unknown_started_turn", "pi").await;
    bind_runtime(
        &state,
        "sess_unknown_started_turn",
        "rtinst_unknown_started_turn",
    )
    .await;

    let (status, body) = post_event(
        state,
        json!({
            "session_id": "sess_unknown_started_turn",
            "turn_id": "turn_client_chosen",
            "type": "turn.started",
            "data": { "runtime_instance_id": "rtinst_unknown_started_turn" }
        }),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "{body:?}");
    assert_eq!(body["error"]["code"], "invalid_request");
}

#[tokio::test]
async fn internal_event_api_allows_started_fact_to_reference_an_existing_turn() {
    let state = test_state().await;
    create_session(&state, "sess_existing_started_turn", "pi").await;
    bind_runtime(
        &state,
        "sess_existing_started_turn",
        "rtinst_existing_started_turn",
    )
    .await;

    let turn_id = new_turn_id().to_string();
    EventIngestService::new(state.db())
        .ingest_reported_event(ReportedEvent::new(
            new_event_id().to_string(),
            "sess_existing_started_turn".to_string(),
            Some(turn_id.clone()),
            EventSource::ExternalApi,
            "pi".to_string(),
            EventType::TurnCreated,
            json!({}),
        ))
        .await
        .expect("create Pontia-owned turn");

    let (referenced_status, referenced) = post_event(
        state,
        json!({
            "session_id": "sess_existing_started_turn",
            "turn_id": turn_id,
            "type": "turn.started",
            "data": { "runtime_instance_id": "rtinst_existing_started_turn" }
        }),
    )
    .await;

    assert_eq!(referenced_status, StatusCode::OK, "{referenced:?}");
    assert_eq!(referenced["turn_id"], turn_id);
}

#[tokio::test]
async fn internal_event_api_rejects_other_creation_facts_with_unknown_supplied_turn_ids() {
    let state = test_state().await;
    create_session(&state, "sess_unknown_created_turn", "generic").await;

    for fact_type in ["turn.created", "turn.queued"] {
        let (status, body) = post_event(
            state.clone(),
            json!({
                "session_id": "sess_unknown_created_turn",
                "turn_id": format!("turn_client_chosen_{fact_type}"),
                "type": fact_type,
                "data": {}
            }),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body:?}");
        assert_eq!(body["error"]["code"], "invalid_request");
    }
}

#[tokio::test]
async fn internal_event_api_uses_returned_turn_id_for_followup_facts() {
    let state = test_state().await;
    create_session(&state, "sess_followup", "pi").await;
    bind_runtime(&state, "sess_followup", "rtinst_followup").await;
    let (_, started) = post_event(
        state.clone(),
        json!({
            "session_id": "sess_followup",
            "type": "turn.started",
            "data": { "runtime_instance_id": "rtinst_followup" }
        }),
    )
    .await;
    let turn_id = started["turn_id"].as_str().expect("turn id");

    for (fact_type, data) in [
        ("turn.output", json!({"output_summary":"answer"})),
        (
            "turn.completed",
            json!({"runtime_instance_id":"rtinst_followup","terminal_leaf_id":null}),
        ),
    ] {
        let (status, body) = post_event(
            state.clone(),
            json!({
                "session_id": "sess_followup",
                "turn_id": turn_id,
                "type": fact_type,
                "data": data
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body:?}");
        assert_eq!(body["turn_id"], turn_id);
    }

    let turn = EventIngestService::new(state.db())
        .get_turn(turn_id)
        .await
        .expect("turn query")
        .expect("turn");
    assert_eq!(turn.output_summary.as_deref(), Some("answer"));
    assert_eq!(turn.state.to_string(), "completed");
}

#[tokio::test]
async fn internal_event_api_accepts_agent_client_reported_turn_interrupted() {
    let state = test_state().await;
    create_session(&state, "sess_interrupted", "pi").await;
    bind_runtime(&state, "sess_interrupted", "rtinst_interrupted").await;

    let (started_status, started) = post_event(
        state.clone(),
        json!({
            "session_id": "sess_interrupted",
            "type": "turn.started",
            "data": { "runtime_instance_id": "rtinst_interrupted" }
        }),
    )
    .await;
    assert_eq!(started_status, StatusCode::OK, "{started:?}");
    let turn_id = started["turn_id"].as_str().expect("turn id");

    let (status, body) = post_event(
        state.clone(),
        json!({
            "session_id": "sess_interrupted",
            "turn_id": turn_id,
            "type": "turn.interrupted",
            "data": { "runtime_instance_id": "rtinst_interrupted" }
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");

    let turn = EventIngestService::new(state.db())
        .get_turn(turn_id)
        .await
        .expect("turn query")
        .expect("turn");
    assert_eq!(turn.state.to_string(), "interrupted");
}

#[tokio::test]
async fn internal_event_api_derives_client_type_and_source_from_session_and_fact() {
    let state = test_state().await;
    create_session(&state, "sess_ready", "pi").await;
    bind_runtime(&state, "sess_ready", "rtinst_ready").await;

    let (status, body) = post_event(
        state.clone(),
        json!({
            "session_id": "sess_ready",
            "type": "session.ready",
            "data": {
                "runtime_instance_id": "rtinst_ready",
                "client_session_key": "native-pi-session"
            }
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");

    let events = EventIngestService::new(state.db())
        .list_events("sess_ready")
        .await
        .expect("events");
    let ready = events.last().expect("ready");
    assert_eq!(ready.source, EventSource::AgentClient);
    assert_eq!(ready.client_type, "pi");
}

#[tokio::test]
async fn internal_event_api_rejects_unknown_sessions_and_missing_followup_turn_ids() {
    let state = test_state().await;
    let (status, _) = post_event(
        state.clone(),
        json!({"session_id":"sess_unknown","type":"session.ready","data":{}}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    create_session(&state, "sess_missing_turn", "generic").await;
    let (status, body) = post_event(
        state,
        json!({"session_id":"sess_missing_turn","type":"turn.output","data":{}}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body:?}");
}

#[tokio::test]
async fn internal_event_api_rejects_followups_for_unknown_or_other_session_turns() {
    let state = test_state().await;
    create_session(&state, "sess_turn_owner", "pi").await;
    create_session(&state, "sess_turn_intruder", "pi").await;
    bind_runtime(&state, "sess_turn_owner", "rtinst_owner").await;
    bind_runtime(&state, "sess_turn_intruder", "rtinst_intruder").await;

    let (unknown_status, unknown_body) = post_event(
        state.clone(),
        json!({
            "session_id": "sess_turn_owner",
            "turn_id": "turn_missing",
            "type": "turn.completed",
            "data": {}
        }),
    )
    .await;
    assert_eq!(unknown_status, StatusCode::CONFLICT, "{unknown_body:?}");

    let (started_status, started) = post_event(
        state.clone(),
        json!({
            "session_id": "sess_turn_owner",
            "type": "turn.started",
            "data": { "runtime_instance_id": "rtinst_owner" }
        }),
    )
    .await;
    assert_eq!(started_status, StatusCode::OK, "{started:?}");
    let turn_id = started["turn_id"].as_str().unwrap();
    let (cross_session_status, cross_session_body) = post_event(
        state,
        json!({
            "session_id": "sess_turn_intruder",
            "turn_id": turn_id,
            "type": "turn.output",
            "data": { "output_summary": "not mine" }
        }),
    )
    .await;
    assert_eq!(
        cross_session_status,
        StatusCode::CONFLICT,
        "{cross_session_body:?}"
    );
}

#[tokio::test]
async fn internal_event_api_rejects_client_owned_domain_fields() {
    let state = test_state().await;
    create_session(&state, "sess_owned_fields", "generic").await;
    let (status, body) = post_event(
        state,
        json!({
            "event_id": "evt_client",
            "session_id": "sess_owned_fields",
            "source": "agent_client",
            "client_type": "generic",
            "type": "session.message_updated",
            "time": "2026-01-01T00:00:00Z",
            "data": {}
        }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body:?}");
}

#[tokio::test]
async fn internal_event_api_validates_context_usage_and_truncates_output() {
    let state = test_state().await;
    create_session(&state, "sess_validation", "generic").await;

    let (status, _) = post_event(
        state.clone(),
        json!({
            "session_id":"sess_validation",
            "type":"session.context_usage_updated",
            "data":{"context_usage":{"usage_ratio":2}}
        }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (_, started) = post_event(
        state.clone(),
        json!({"session_id":"sess_validation","type":"turn.started","data":{}}),
    )
    .await;
    let turn_id = started["turn_id"].as_str().expect("turn id");
    let (status, body) = post_event(
        state.clone(),
        json!({
            "session_id":"sess_validation",
            "turn_id":turn_id,
            "type":"turn.output",
            "data":{"output":{"summary":"x".repeat(500)}}
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");
    let turn = EventIngestService::new(state.db())
        .get_turn(turn_id)
        .await
        .expect("turn query")
        .expect("turn");
    assert_eq!(turn.output_summary.expect("summary").chars().count(), 200);
}

#[tokio::test]
async fn internal_event_api_keeps_session_message_updates_volatile() {
    let state = test_state().await;
    create_session(&state, "sess_volatile", "generic").await;
    let mut agent_events = state.agent_events().subscribe();
    let before = EventIngestService::new(state.db())
        .list_events("sess_volatile")
        .await
        .expect("events")
        .len();
    let (status, body) = post_event(
        state.clone(),
        json!({
            "session_id":"sess_volatile",
            "type":"session.message_updated",
            "data":{"reason":"append"}
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");
    let after = EventIngestService::new(state.db())
        .list_events("sess_volatile")
        .await
        .expect("events")
        .len();
    assert_eq!(after, before);
    assert!(matches!(
        agent_events.try_recv(),
        Err(tokio::sync::broadcast::error::TryRecvError::Empty)
    ));
}
