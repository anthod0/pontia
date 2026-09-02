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

use crate::common::test_app::TestApp;

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
                "phase": "Control",
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
                    "phase": "  Discovery  ",
                    "title": "Research",
                    "instructions": "Research the implementation.",
                    "inputs": ["requirements.md"],
                    "output": "research.md",
                    "execution_profile_id": null,
                    "execution_profile_version": null
                },
                {
                    "type": "agent",
                    "phase": "Delivery",
                    "title": "Implement",
                    "instructions": "Implement the feature.",
                    "inputs": ["research.md"],
                    "output": "result.md",
                    "execution_profile_id": null,
                    "execution_profile_version": null
                },
                {
                    "type": "agent",
                    "phase": "Discovery",
                    "title": "Review",
                    "instructions": "Review the result.",
                    "inputs": ["result.md"],
                    "output": "review.md",
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
    assert_eq!(nodes.len(), 3);
    assert_eq!(nodes[0].node_type, "agent");
    assert_eq!(nodes[0].phase, "Discovery");
    assert_eq!(nodes[1].phase, "Delivery");
    assert_eq!(nodes[2].phase, "Discovery");
    assert_eq!(
        nodes[1].parent_node_id.as_deref(),
        Some(nodes[0].node_id.as_str())
    );
    assert_eq!(
        nodes[2].parent_node_id.as_deref(),
        Some(nodes[1].node_id.as_str())
    );
    assert_eq!(
        nodes[0].session_id.as_deref(),
        body["data"]["session_id"].as_str()
    );
    let workflow_dir = app.pontia_home().path().join("workflows/wf_http_run");
    assert_eq!(
        fs::read_to_string(workflow_dir.join("handoff/requirements.md"))
            .expect("read initial handoff"),
        "Build it.\n"
    );
    let definition = fs::read_to_string(workflow_dir.join("workflow.toml"))
        .expect("read durable Workflow definition");
    assert!(definition.contains("workflow_id = \"wf_http_run\""));
    assert!(definition.contains("revision = 1"));
    assert!(definition.contains("title = \"HTTP Workflow run\""));
    assert!(definition.contains(&format!("cwd = {cwd:?}")));
    assert!(definition.contains("source = \"handoff/requirements.md\""));
    assert!(definition.contains("type = \"agent\""));
    assert!(definition.contains("phase = \"Discovery\""));
    for node in &nodes {
        assert!(
            definition.contains(&format!("id = {:?}", node.node_id)),
            "durable definition must contain accepted Node identity {}",
            node.node_id
        );
    }
    assert!(!workflow_dir.join(".workflow.toml.tmp").exists());
}

#[tokio::test]
async fn internal_workflow_run_requires_a_valid_phase() {
    for (workflow_id, phase, expected_message) in [
        ("wf_empty_phase", "", "Agent Node phase must not be empty"),
        (
            "wf_blank_phase",
            "   ",
            "Agent Node phase must not be empty",
        ),
        (
            "wf_long_phase",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "Agent Node phase must be at most 80 characters",
        ),
    ] {
        let app = TestApp::new().await;
        let (status, body) = post_run(
            &app,
            json!({
                "workflow_id": workflow_id,
                "title": "Invalid phase",
                "cwd": app.workspace().path().display().to_string(),
                "nodes": [{
                    "type": "agent",
                    "phase": phase,
                    "title": "Worker",
                    "instructions": "Do the work.",
                    "inputs": [],
                    "output": "result.md",
                    "execution_profile_id": null,
                    "execution_profile_version": null
                }]
            }),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        assert!(
            body["error"]["message"]
                .as_str()
                .is_some_and(|message| message.contains(expected_message)),
            "{body}"
        );
    }
}

#[tokio::test]
async fn internal_workflow_run_rejects_a_missing_phase_during_json_decoding() {
    let app = TestApp::new().await;
    let (status, body) = post_run(
        &app,
        json!({
            "workflow_id": "wf_missing_phase",
            "title": "Missing phase",
            "cwd": app.workspace().path().display().to_string(),
            "nodes": [{
                "type": "agent",
                "title": "Worker",
                "instructions": "Do the work.",
                "inputs": [],
                "output": "result.md",
                "execution_profile_id": null,
                "execution_profile_version": null
            }]
        }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(
        body["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("missing field `phase`")),
        "{body}"
    );
}
