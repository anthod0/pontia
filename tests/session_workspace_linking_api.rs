#[path = "support/generic_client.rs"]
mod generic_client;
#[path = "support/http.rs"]
mod http;
#[path = "support/task_state.rs"]
mod task_state;

use axum::http::StatusCode;
use generic_client::GenericClientTestScope;
use http::{get_json, post_json};
use serde_json::json;
use task_state::test_state;

#[tokio::test]
async fn create_session_upserts_canonical_workspace_and_links_session() {
    let _scope = GenericClientTestScope::new().await;
    let state = test_state().await;
    let workspace = tempfile::tempdir().expect("workspace");
    let canonical = std::fs::canonicalize(workspace.path()).expect("canonical");

    let (status, body) = post_json(
        state.clone(),
        "/external/v1/sessions",
        json!({"client_type":"generic", "workspace": workspace.path().display().to_string()}),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    assert!(body["data"]["session"]["session_id"].as_str().is_some());
    assert_eq!(
        body["data"]["session"]["workspace"],
        canonical.display().to_string()
    );
    let workspace_id = body["data"]["session"]["workspace_id"]
        .as_str()
        .expect("workspace id");
    assert!(workspace_id.starts_with("wks_"));

    let (status, body) = get_json(state, "/external/v1/workspaces").await;
    assert_eq!(status, StatusCode::OK);
    let workspaces = body["data"]["workspaces"].as_array().unwrap();
    assert_eq!(workspaces.len(), 1);
    assert_eq!(workspaces[0]["workspace_id"], workspace_id);
    assert_eq!(
        workspaces[0]["canonical_path"],
        canonical.display().to_string()
    );
    assert_eq!(workspaces[0]["state"], "active");
}
