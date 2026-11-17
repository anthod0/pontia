use crate::{
    common::generic_client::GenericClientTestScope,
    support::http::{get_json, post_json},
};
use axum::http::StatusCode;
use serde_json::json;

#[tokio::test]
async fn create_session_links_existing_canonical_workspace() {
    let _scope = GenericClientTestScope::new().await;
    let state = crate::common::test_app::TestApp::builder()
        .external_api_token(Some(crate::support::http::TOKEN.to_string()))
        .build_state()
        .await;
    let workspace = crate::common::test_app::registered_workspace(&state).await;
    let canonical = std::fs::canonicalize(workspace.path()).expect("canonical");

    let (status, body) = post_json(
        state.clone(),
        "/api/v1/sessions",
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

    let (status, body) = get_json(state, "/api/v1/workspaces").await;
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
