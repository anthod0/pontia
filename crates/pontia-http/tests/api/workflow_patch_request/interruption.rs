use std::sync::{Arc, Mutex};

use pontia_application::{AgentEventBroker, CreateSessionRequest, EventIngestService};
use pontia_workflow::{
    GracefulExitRequester, SessionCreator, TurnInterruptionRequester, WorkflowCoordinator,
    WorkflowQueryService,
};

use super::*;

struct ReplannerCreator(sqlx::SqlitePool);

impl SessionCreator for ReplannerCreator {
    async fn create_session(
        &self,
        request: CreateSessionRequest,
    ) -> pontia_workflow::Result<String> {
        assert_eq!(request.role.as_deref(), Some("workflow_replanner"));
        sqlx::query(
            "INSERT INTO sessions (session_id, client_type, state, metadata) VALUES ('sess_patch_replanner', 'pi', 'idle', ?)",
        )
        .bind(request.metadata.to_string())
        .execute(&self.0)
        .await
        .expect("create dedicated Re-planner Session");
        sqlx::query(
            "INSERT INTO runtime_bindings (session_id, runtime_kind, runtime_instance_id, binding_state) VALUES ('sess_patch_replanner', 'pi_tui', 'runtime_patch_replanner', 'confirmed')",
        )
        .execute(&self.0)
        .await
        .expect("bind Re-planner Runtime");
        Ok("sess_patch_replanner".into())
    }
}

#[derive(Clone, Default)]
struct RuntimeControl {
    exits: Arc<Mutex<Vec<(String, String)>>>,
}

impl TurnInterruptionRequester for RuntimeControl {
    async fn request_turn_interruption(
        &self,
        session_id: &str,
        turn_id: &str,
        runtime_instance_id: &str,
    ) -> pontia_workflow::Result<()> {
        assert_eq!(session_id, "sess_patch_request");
        assert_eq!(turn_id, "turn_patch_request");
        assert_eq!(runtime_instance_id, "runtime_patch_request");
        Ok(())
    }
}

impl GracefulExitRequester for RuntimeControl {
    async fn ensure_current_runtime(&self, _: &str, _: &str) -> pontia_workflow::Result<()> {
        panic!("requester must remain alive during replanning")
    }

    async fn request_graceful_exit(
        &self,
        session_id: &str,
        runtime: &str,
    ) -> pontia_workflow::Result<()> {
        self.exits
            .lock()
            .unwrap()
            .push((session_id.into(), runtime.into()));
        Ok(())
    }
}

#[tokio::test]
async fn reported_requester_interruption_starts_one_replanner() {
    let app = TestApp::new().await;
    seed_requester(&app).await;
    let repository = SqliteWorkflowRepository::new(app.db.clone());
    report_requester_fact(
        &app,
        "turn.started",
        json!({ "runtime_instance_id": "runtime_patch_request" }),
    )
    .await;
    let (status, requested) = request_patch(&app, "runtime_patch_request").await;
    assert_eq!(status, StatusCode::OK, "{requested}");
    let patch_id = requested["data"]["patch_id"].as_str().unwrap();
    let coordinator = WorkflowCoordinator::with_services(
        app.db.clone(),
        ReplannerCreator(app.db.clone()),
        RuntimeControl::default(),
        app.state.agent_events(),
        app.pontia_home().path().to_path_buf(),
    );

    coordinator.reconcile("wf_patch_request").await.unwrap();
    let patch = repository.get_patch(patch_id).await.unwrap().unwrap();
    assert_eq!(patch.state, "requested");
    assert!(patch.replanner_session_id.is_none());

    report_requester_fact(
        &app,
        "turn.interrupted",
        json!({ "terminal_leaf_id": null }),
    )
    .await;
    let turn = EventIngestService::new(app.db.clone())
        .get_turn("turn_patch_request")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(turn.state.to_string(), "interrupted");

    for _ in 0..2 {
        coordinator.reconcile("wf_patch_request").await.unwrap();
    }
    let patch = repository.get_patch(patch_id).await.unwrap().unwrap();
    assert_eq!(
        patch.state, "planning",
        "confirmed interruption must unlock replanning"
    );
    assert_eq!(
        patch.replanner_session_id.as_deref(),
        Some("sess_patch_replanner")
    );
    let view = WorkflowQueryService::new(app.db.clone())
        .get_workflow("wf_patch_request")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        serde_json::to_value(&view).unwrap()["nodes"][0]["status"],
        "paused"
    );
    assert_eq!(
        repository
            .get_workflow("wf_patch_request")
            .await
            .unwrap()
            .unwrap()
            .state,
        "replanning"
    );
    assert!(
        repository
            .list_events("wf_patch_request")
            .await
            .unwrap()
            .iter()
            .all(|event| event.event_type != "workflow.failed")
    );
}

#[tokio::test]
async fn submitted_current_node_remains_exiting_after_interruption() {
    let app = TestApp::new().await;
    seed_requester(&app).await;
    report_requester_fact(
        &app,
        "turn.started",
        json!({ "runtime_instance_id": "runtime_patch_request" }),
    )
    .await;
    SqliteWorkflowRepository::new(app.db.clone())
        .record_node_submission(
            "node_patch_request",
            "runtime_patch_request",
            "evt_submission",
        )
        .await
        .unwrap();
    report_requester_fact(
        &app,
        "turn.interrupted",
        json!({ "terminal_leaf_id": null }),
    )
    .await;
    let view = WorkflowQueryService::new(app.db.clone())
        .get_workflow("wf_patch_request")
        .await
        .unwrap()
        .unwrap();
    let view = serde_json::to_value(view).unwrap();
    assert_eq!(view["state"], "running");
    assert_eq!(view["nodes"][0]["status"], "exiting");
}

async fn report_requester_fact(app: &TestApp, fact_type: &str, data: Value) {
    report_fact(
        app,
        "sess_patch_request",
        Some("turn_patch_request"),
        fact_type,
        data,
    )
    .await;
}

async fn report_fact(
    app: &TestApp,
    session_id: &str,
    turn_id: Option<&str>,
    fact_type: &str,
    data: Value,
) -> Value {
    let response = http::router(app.state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/internal/v1/events")
                .header(header::AUTHORIZATION, "Bearer test-token")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "session_id": session_id,
                        "turn_id": turn_id,
                        "type": fact_type,
                        "data": data
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    serde_json::from_slice(&body).unwrap()
}

type Coordinator =
    WorkflowCoordinator<ReplannerCreator, RuntimeControl, RuntimeControl, AgentEventBroker>;

async fn planning_patch(app: &TestApp, control: RuntimeControl) -> (String, Coordinator, String) {
    seed_requester(app).await;
    report_requester_fact(
        app,
        "turn.started",
        json!({ "runtime_instance_id": "runtime_patch_request" }),
    )
    .await;
    let (status, requested) = request_patch(app, "runtime_patch_request").await;
    assert_eq!(status, StatusCode::OK, "{requested}");
    let patch_id = requested["data"]["patch_id"].as_str().unwrap().to_string();
    report_requester_fact(app, "turn.interrupted", json!({ "terminal_leaf_id": null })).await;
    let coordinator = WorkflowCoordinator::with_services(
        app.db.clone(),
        ReplannerCreator(app.db.clone()),
        control,
        app.state.agent_events(),
        app.pontia_home().path().to_path_buf(),
    );
    coordinator.reconcile("wf_patch_request").await.unwrap();
    let started = report_fact(
        app,
        "sess_patch_replanner",
        None,
        "turn.started",
        json!({ "runtime_instance_id": "runtime_patch_replanner" }),
    )
    .await;
    (
        patch_id,
        coordinator,
        started["turn_id"].as_str().unwrap().into(),
    )
}

#[tokio::test]
async fn reported_replanner_terminal_blocks_an_unresolved_patch() {
    for terminal in ["turn.completed", "turn.failed", "turn.interrupted"] {
        let app = TestApp::new().await;
        let control = RuntimeControl::default();
        let (patch_id, coordinator, turn_id) = planning_patch(&app, control.clone()).await;
        report_fact(
            &app,
            "sess_patch_replanner",
            Some(&turn_id),
            terminal,
            json!({ "terminal_leaf_id": null, "failure_message": "cannot finish" }),
        )
        .await;
        coordinator.reconcile("wf_patch_request").await.unwrap();
        let repository = SqliteWorkflowRepository::new(app.db.clone());
        let patch = repository.get_patch(&patch_id).await.unwrap().unwrap();
        assert_eq!(
            patch.state, "blocked",
            "{terminal} must settle the unresolved Patch"
        );
        assert_eq!(
            repository
                .get_workflow("wf_patch_request")
                .await
                .unwrap()
                .unwrap()
                .state,
            "blocked"
        );
        coordinator.reconcile("wf_patch_request").await.unwrap();
        coordinator.reconcile("wf_patch_request").await.unwrap();
        assert_eq!(
            control.exits.lock().unwrap().as_slice(),
            &[(
                "sess_patch_replanner".into(),
                "runtime_patch_replanner".into()
            )]
        );
    }
}

#[tokio::test]
async fn applied_and_rejected_patches_do_not_fail_on_the_requester_interruption() {
    for changed in [false, true] {
        let app = TestApp::new().await;
        let control = RuntimeControl::default();
        let (patch_id, coordinator, turn_id) = planning_patch(&app, control.clone()).await;
        if changed {
            let file = app
                .pontia_home()
                .path()
                .join("workflows/wf_patch_request/workflow.toml");
            let mut definition = fs::read_to_string(&file).unwrap();
            definition.push_str("\n[[nodes]]\ntype = \"agent\"\nphase = \"Validate\"\ntitle = \"New node\"\ninstructions = \"Validate the result\"\ninputs = [\"result.md\"]\noutput = \"verified.md\"\n");
            fs::write(file, definition).unwrap();
        }
        let (status, resolved) =
            apply_patch(&app, "runtime_patch_replanner", "Planning decision").await;
        assert_eq!(status, StatusCode::OK, "{resolved}");
        assert_eq!(
            resolved["data"]["outcome"],
            if changed { "applied" } else { "rejected" }
        );
        assert_eq!(resolved["data"]["revision"], if changed { 2 } else { 1 });
        for _ in 0..3 {
            coordinator.reconcile("wf_patch_request").await.unwrap();
        }
        let repository = SqliteWorkflowRepository::new(app.db.clone());
        let patch = repository.get_patch(&patch_id).await.unwrap().unwrap();
        assert!(patch.continuation_queued_at.is_some());
        assert!(
            control.exits.lock().unwrap().is_empty(),
            "resolution is not a Turn terminal fact"
        );
        let view = WorkflowQueryService::new(app.db.clone())
            .get_workflow("wf_patch_request")
            .await
            .unwrap()
            .unwrap();
        let view = serde_json::to_value(view).unwrap();
        assert_eq!(view["state"], "running");
        assert_eq!(view["nodes"][0]["status"], "paused");
        report_fact(
            &app,
            "sess_patch_replanner",
            Some(&turn_id),
            "turn.completed",
            json!({ "terminal_leaf_id": null }),
        )
        .await;
        for _ in 0..2 {
            coordinator.reconcile("wf_patch_request").await.unwrap();
        }
        assert_eq!(
            control.exits.lock().unwrap().as_slice(),
            &[(
                "sess_patch_replanner".into(),
                "runtime_patch_replanner".into()
            )]
        );
        assert_eq!(
            repository
                .get_workflow("wf_patch_request")
                .await
                .unwrap()
                .unwrap()
                .state,
            "running"
        );
    }
}

#[tokio::test]
async fn explicitly_blocked_patch_stays_stopped_and_closes_replanner_after_terminal_fact() {
    let app = TestApp::new().await;
    let control = RuntimeControl::default();
    let (patch_id, coordinator, turn_id) = planning_patch(&app, control.clone()).await;
    let file = app
        .pontia_home()
        .path()
        .join("workflows/wf_patch_request/workflow.toml");
    let accepted = fs::read_to_string(&file).unwrap();
    fs::write(&file, "invalid draft").unwrap();
    let (status, blocked) = block_patch(
        &app,
        "runtime_patch_replanner",
        "Cannot delete the protected node",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{blocked}");
    coordinator.reconcile("wf_patch_request").await.unwrap();
    assert!(control.exits.lock().unwrap().is_empty());
    let repository = SqliteWorkflowRepository::new(app.db.clone());
    let patch = repository.get_patch(&patch_id).await.unwrap().unwrap();
    assert_eq!(patch.state, "blocked");
    assert!(patch.continuation_message_id.is_none());
    assert!(patch.blocked_draft_ref.is_some());
    assert_eq!(fs::read_to_string(&file).unwrap(), accepted);
    let view = WorkflowQueryService::new(app.db.clone())
        .get_workflow("wf_patch_request")
        .await
        .unwrap()
        .unwrap();
    let view = serde_json::to_value(view).unwrap();
    assert_eq!(view["state"], "blocked");
    assert_eq!(view["nodes"][0]["status"], "paused");
    report_fact(
        &app,
        "sess_patch_replanner",
        Some(&turn_id),
        "turn.completed",
        json!({ "terminal_leaf_id": null }),
    )
    .await;
    for _ in 0..2 {
        coordinator.reconcile("wf_patch_request").await.unwrap();
    }
    assert_eq!(
        control.exits.lock().unwrap().as_slice(),
        &[(
            "sess_patch_replanner".into(),
            "runtime_patch_replanner".into()
        )]
    );
    assert_eq!(
        repository
            .get_workflow("wf_patch_request")
            .await
            .unwrap()
            .unwrap()
            .state,
        "blocked"
    );
}
