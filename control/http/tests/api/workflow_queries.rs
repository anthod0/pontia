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

fn node(id: &str, parent: Option<&str>, phase: &str, title: &str) -> CreateWorkflowNodeRecord {
    CreateWorkflowNodeRecord {
        node_id: id.to_string(),
        workflow_id: "wf_observe".to_string(),
        parent_node_id: parent.map(str::to_string),
        phase: phase.to_string(),
        title: title.to_string(),
        instructions: "Observe it".to_string(),
        inputs: "[]".to_string(),
        output: format!("{id}.md"),
        execution_profile_id: None,
        execution_profile_version: None,
    }
}

async fn get(app: &TestApp, uri: &str) -> (StatusCode, Value) {
    let response = http::router(app.state.clone())
        .oneshot(
            Request::builder()
                .uri(uri)
                .header(header::AUTHORIZATION, "Bearer test-token")
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

async fn create_observable_workflow(app: &TestApp) {
    SqliteWorkflowRepository::new(app.db.clone())
        .create_definition(
            CreateWorkflowRecord {
                workflow_id: "wf_observe".to_string(),
                title: "Observe workflow".to_string(),
                cwd: app.workspace().path().display().to_string(),
                state: "pending".to_string(),
            },
            vec![
                node("node_a", None, "Research", "Research one"),
                node("node_b", Some("node_a"), "Research", "Research two"),
                node("node_c", Some("node_b"), "Review", "Review"),
                node("node_d", Some("node_c"), "Research", "Research again"),
            ],
        )
        .await
        .expect("create workflow");
}

#[tokio::test]
async fn external_workflow_queries_return_ordered_nodes_for_frontend_phase_grouping() {
    let app = TestApp::new().await;
    create_observable_workflow(&app).await;

    let (list_status, list) = get(&app, "/external/v1/workflows").await;
    assert_eq!(list_status, StatusCode::OK, "{list}");
    assert_eq!(list["data"]["workflows"][0]["workflow_id"], "wf_observe");
    assert_eq!(
        list["data"]["workflows"][0]["current_phase_name"],
        "Research"
    );

    let (detail_status, detail) = get(&app, "/external/v1/workflows/wf_observe").await;
    assert_eq!(detail_status, StatusCode::OK, "{detail}");
    let nodes = detail["data"]["workflow"]["nodes"]
        .as_array()
        .expect("nodes");
    assert_eq!(nodes.len(), 4);
    assert_eq!(nodes[0]["node_id"], "node_a");
    assert_eq!(nodes[1]["phase"], "Research");
    assert_eq!(nodes[2]["phase"], "Review");
    assert_eq!(nodes[3]["phase"], "Research");
    assert_eq!(nodes[0]["status"], "pending");
}

#[tokio::test]
async fn external_workflow_queries_reject_invalid_limits() {
    let app = TestApp::new().await;
    for value in ["0", "101", "nope", "-1"] {
        let (status, body) = get(&app, &format!("/external/v1/workflows?limit={value}")).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        assert_eq!(body["error"]["code"], "invalid_request");
    }
}

#[tokio::test]
async fn invalid_legacy_phase_is_visible_in_list_but_detail_conflicts() {
    let app = TestApp::new().await;
    SqliteWorkflowRepository::new(app.db.clone())
        .create_definition(
            CreateWorkflowRecord {
                workflow_id: "wf_observe".to_string(),
                title: "Legacy workflow".to_string(),
                cwd: app.workspace().path().display().to_string(),
                state: "pending".to_string(),
            },
            vec![node("node_a", None, "", "Legacy")],
        )
        .await
        .expect("create legacy workflow");

    let (list_status, list) = get(&app, "/external/v1/workflows").await;
    assert_eq!(list_status, StatusCode::OK, "{list}");
    assert_eq!(
        list["data"]["workflows"][0]["observation_error"],
        "invalid_definition"
    );

    let (detail_status, detail) = get(&app, "/external/v1/workflows/wf_observe").await;
    assert_eq!(detail_status, StatusCode::CONFLICT, "{detail}");
    assert_eq!(detail["error"]["code"], "state_conflict");
}
