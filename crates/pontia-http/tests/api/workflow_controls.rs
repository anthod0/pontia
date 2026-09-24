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

    let (pause_status, pause) =
        post(&app, "/api/v1/workflows/wf_control/pause", "pause-once").await;
    assert_eq!(pause_status, StatusCode::OK, "{pause}");
    assert_eq!(pause["data"]["workflow"]["state"], "paused");
    assert_eq!(pause["data"]["control"]["interrupt_requested"], false);

    let (retry_status, retry) =
        post(&app, "/api/v1/workflows/wf_control/pause", "pause-once").await;
    assert_eq!(retry_status, StatusCode::OK, "{retry}");
    assert_eq!(retry["data"], pause["data"]);

    let (resume_status, resume) =
        post(&app, "/api/v1/workflows/wf_control/resume", "resume-once").await;
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
        "/api/v1/workflows/wf_control/resume",
        "resume-running",
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "state_conflict");
}

#[tokio::test]
async fn workflow_retry_is_authenticated_and_persistently_deduplicated_by_failure() {
    let app = TestApp::new().await;
    let repo = seed_running_workflow(&app).await;
    sqlx::query("INSERT INTO sessions(session_id,client_type,state) VALUES ('failed-session','pi','exited')").execute(&app.db).await.unwrap();
    sqlx::query("INSERT INTO agent_bindings(id,session_id,client_type,launch_cwd,client_session_key) VALUES ('binding','failed-session','pi','/workspace','native')").execute(&app.db).await.unwrap();
    repo.bind_node_session("node_control", "failed-session")
        .await
        .unwrap();
    sqlx::query("INSERT INTO events(event_id,session_id,source,client_type,event_type,occurred_at,payload) VALUES ('exit','failed-session','runtime_manager','pi','session.exited','2026-09-24T00:00:00Z','{\"runtime_instance_id\":\"old\"}')").execute(&app.db).await.unwrap();
    repo.fail_unsubmitted_workflow_node(
        "wf_control",
        "node_control",
        "failure",
        "Agent Client reported session.exited before Agent Node node_control Submission",
        "exit",
        Some("old"),
    )
    .await
    .unwrap();
    for (token, status) in [
        ("wrong", StatusCode::UNAUTHORIZED),
        ("test-token", StatusCode::OK),
        ("test-token", StatusCode::OK),
    ] {
        let response = http::router(app.state.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/workflows/wf_control/retry")
                    .header(header::AUTHORIZATION, format!("Bearer {token}"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"failure_event_id":"failure"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), status);
    }
    assert_eq!(repo.list_recoveries("wf_control").await.unwrap().len(), 1);
    assert_eq!(
        repo.get_workflow("wf_control")
            .await
            .unwrap()
            .unwrap()
            .state,
        "recovering"
    );
}
