use std::fs;

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use pontia_http as http;
use pontia_storage_sqlite::repositories::workflows::SqliteWorkflowRepository;
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::test_app::TestApp;

async fn post_run(app: &TestApp, body: Value) -> (StatusCode, Value) {
    let response = http::router(app.state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/internal/v1/workflows")
                .header(header::AUTHORIZATION, "Bearer test-token")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .expect("request"),
        )
        .await
        .expect("response");
    let status = response.status();
    let body = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    (
        status,
        serde_json::from_slice(&body).expect("JSON response"),
    )
}

#[tokio::test]
async fn internal_workflow_run_rejects_unsupported_node_types_before_creation() {
    let app = TestApp::new().await;

    let (status, body) = post_run(
        &app,
        json!({
            "workflow_id": "wf_unsupported_type",
            "title": "Unsupported node",
            "cwd": app.workspace().path().display().to_string(),
            "nodes": [{
                "type": "control",
                "title": "Sequence",
                "instructions": "Run children.",
                "inputs": [],
                "output": "result.md"
            }]
        }),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["error"]["code"], "invalid_request");
    assert!(
        body["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("unsupported Workflow Node type: control"))
    );
    assert!(
        SqliteWorkflowRepository::new(app.db.clone())
            .get_workflow("wf_unsupported_type")
            .await
            .expect("load workflow")
            .is_none()
    );
}

#[tokio::test]
async fn internal_workflow_run_creates_and_starts_a_linear_agent_workflow() {
    let app = TestApp::builder().pi_runtime_stub(true).build().await;
    let cwd = app.workspace().path().display().to_string();

    let (status, body) = post_run(
        &app,
        json!({
            "workflow_id": "wf_http_run",
            "title": "HTTP Workflow run",
            "cwd": cwd,
            "handoffs": [{"name": "requirements.md", "content": "Build it.\n"}],
            "nodes": [
                {
                    "type": "agent",
                    "title": "Research",
                    "instructions": "Research the implementation.",
                    "inputs": ["requirements.md"],
                    "output": "research.md",
                    "execution_profile_id": null,
                    "execution_profile_version": null
                },
                {
                    "type": "agent",
                    "title": "Implement",
                    "instructions": "Implement the feature.",
                    "inputs": ["research.md"],
                    "output": "result.md",
                    "execution_profile_id": null,
                    "execution_profile_version": null
                }
            ]
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["workflow_id"], "wf_http_run");
    assert!(body["data"]["node_id"].as_str().is_some());
    assert!(body["data"]["session_id"].as_str().is_some());

    let repository = SqliteWorkflowRepository::new(app.db.clone());
    let workflow = repository
        .get_workflow("wf_http_run")
        .await
        .expect("load workflow")
        .expect("workflow exists");
    assert_eq!(workflow.state, "running");
    let nodes = repository
        .list_nodes("wf_http_run")
        .await
        .expect("list nodes");
    assert_eq!(nodes.len(), 2);
    assert_eq!(nodes[0].node_type, "agent");
    assert_eq!(
        nodes[1].parent_node_id.as_deref(),
        Some(nodes[0].node_id.as_str())
    );
    assert_eq!(
        nodes[0].session_id.as_deref(),
        body["data"]["session_id"].as_str()
    );
    assert_eq!(
        fs::read_to_string(
            app.pontia_home()
                .path()
                .join("workflows/wf_http_run/handoff/requirements.md")
        )
        .expect("read initial handoff"),
        "Build it.\n"
    );
}
