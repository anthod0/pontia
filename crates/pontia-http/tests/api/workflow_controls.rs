use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use pontia_http as http;
use pontia_storage_sqlite::repositories::workflows::{
    CreateWorkflowNodeRecord, CreateWorkflowRecord, SqliteWorkflowRepository,
};
use serde_json::Value;
use tower::ServiceExt;

use crate::common::test_app::TestApp;

async fn post(app: &TestApp, uri: &str, key: &str) -> (StatusCode, Value) {
    let response = http::router(app.state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header(header::AUTHORIZATION, "Bearer test-token")
                .header("Idempotency-Key", key)
                .body(Body::empty())
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
    (
        status,
        serde_json::from_slice(&bytes).expect("JSON response"),
    )
}

async fn seed_running_workflow(app: &TestApp) -> SqliteWorkflowRepository {
    let repository = SqliteWorkflowRepository::new(app.db.clone());
    repository
        .create_definition(
            CreateWorkflowRecord {
                workflow_id: "wf_control".to_string(),
                title: "Controlled workflow".to_string(),
                cwd: app.workspace().path().display().to_string(),
                state: "running".to_string(),
            },
            vec![CreateWorkflowNodeRecord {
                node_id: "node_control".to_string(),
                workflow_id: "wf_control".to_string(),
                parent_node_id: None,
                phase: "Build".to_string(),
                title: "Worker".to_string(),
                instructions: "Work".to_string(),
                inputs: "[]".to_string(),
                output: "result.md".to_string(),
                execution_profile_id: None,
                execution_profile_version: None,
            }],
        )
        .await
        .expect("seed workflow");
    repository
}

#[tokio::test]
async fn external_workflow_pause_and_resume_are_persisted_and_idempotent() {
    let app = TestApp::new().await;
    let repository = seed_running_workflow(&app).await;

    let (pause_status, pause) = post(
        &app,
        "/external/v1/workflows/wf_control/pause",
        "pause-once",
    )
    .await;
    assert_eq!(pause_status, StatusCode::OK, "{pause}");
    assert_eq!(pause["data"]["workflow"]["state"], "paused");
    assert_eq!(pause["data"]["control"]["interrupt_requested"], false);

    let (retry_status, retry) = post(
        &app,
        "/external/v1/workflows/wf_control/pause",
        "pause-once",
    )
    .await;
    assert_eq!(retry_status, StatusCode::OK, "{retry}");
    assert_eq!(retry["data"], pause["data"]);

    let (resume_status, resume) = post(
        &app,
        "/external/v1/workflows/wf_control/resume",
        "resume-once",
    )
    .await;
    assert_eq!(resume_status, StatusCode::OK, "{resume}");
    assert_eq!(resume["data"]["workflow"]["state"], "running");
    assert_eq!(resume["data"]["control"]["continue_sent"], false);

    let events = repository
        .list_events("wf_control")
        .await
        .expect("workflow events");
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].event_type, "workflow.paused");
    assert_eq!(events[1].event_type, "workflow.resumed");
}

#[tokio::test]
async fn external_workflow_controls_reject_invalid_source_states() {
    let app = TestApp::new().await;
    seed_running_workflow(&app).await;

    let (status, body) = post(
        &app,
        "/external/v1/workflows/wf_control/resume",
        "resume-running",
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "state_conflict");
}
