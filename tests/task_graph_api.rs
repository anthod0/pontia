#[path = "support/graph_state.rs"]
mod graph_state;
#[path = "support/http.rs"]
mod http;

use axum::http::StatusCode;
use graph_state::graph_planner_test_state;
use http::{get_json, post_json};
use serde_json::json;

#[tokio::test]
async fn graph_enabled_projects_planner_decision_into_agent_collaboration_graph() {
    let graph_dir = tempfile::tempdir().expect("graph dir");
    let graph_path = graph_dir.path().join("lbug");
    let state = graph_planner_test_state(graph_path.display().to_string()).await;
    let workspace = tempfile::tempdir().expect("workspace");
    let canonical = std::fs::canonicalize(workspace.path()).expect("canonical");

    let (status, body) = post_json(
        state.clone(),
        "/external/v1/tasks",
        json!({
            "input":"resolve this workspace with evidence",
            "client_type":"generic",
            "metadata": {
                "planner_decision": {
                    "decision_id":"dec_test_graph",
                    "status":"resolved",
                    "workspace": {
                        "canonical_path": canonical.display().to_string(),
                        "confidence": 0.72,
                        "reason": "fake planner matched graph workspace"
                    },
                    "reason":"fake planner resolved for graph",
                    "evidence":[{
                        "evidence_id":"ev_test_graph",
                        "kind":"heuristic",
                        "ref":"metadata",
                        "summary":"test evidence"
                    }]
                }
            }
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let task_id = body["data"]["task"]["task_id"].as_str().unwrap();

    let (status, provenance) =
        get_json(state, &format!("/external/v1/tasks/{task_id}/provenance")).await;

    assert_eq!(status, StatusCode::OK, "provenance body: {provenance:?}");
    let nodes = provenance["data"]["nodes"].as_array().expect("nodes");
    assert!(
        nodes
            .iter()
            .any(|node| node["kind"] == "Task" && node["id"] == task_id)
    );
    assert!(nodes.iter().any(|node| node["kind"] == "WorkItem"
        && node["id"] == "wi_dec_test_graph"
        && node["properties"]["kind"] == "planning"));
    assert!(nodes.iter().any(|node| node["kind"] == "Agent"
        && node["id"] == "agent_planner"
        && node["properties"]["role"] == "planner"));
    assert!(
        nodes
            .iter()
            .any(|node| node["kind"] == "Artifact" && node["id"] == "art_ev_test_graph")
    );
    assert!(
        nodes
            .iter()
            .any(|node| node["kind"] == "Signal" && node["id"] == "sig_dec_test_graph")
    );
    assert!(!nodes.iter().any(|node| matches!(
        node["kind"].as_str(),
        Some("Workspace" | "Session" | "Turn" | "Decision" | "Evidence")
    )));
    let edges = provenance["data"]["edges"].as_array().expect("edges");
    assert!(edges.iter().any(|edge| edge["kind"] == "HAS_WORK"));
    assert!(edges.iter().any(|edge| edge["kind"] == "HAS_SIGNAL"));
    assert!(edges.iter().any(|edge| edge["kind"] == "ASSIGNED_TO"));
    assert!(edges.iter().any(|edge| edge["kind"] == "REQUIRES"));
    assert!(edges.iter().any(|edge| edge["kind"] == "EMITS"));
    assert!(edges.iter().any(|edge| edge["kind"] == "SUPPORTED_BY"));
    assert!(!edges.iter().any(|edge| matches!(
        edge["kind"].as_str(),
        Some("HAS_DECISION" | "ROUTED_TO" | "DISPATCHED_TO" | "HAS_TURN")
    )));
}
