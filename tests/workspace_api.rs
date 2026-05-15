use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use llmparty::{
    application::{AppState, GraphRuntimeConfig, WorkspaceBrowserConfig, WorkspaceRootConfig},
    storage::sqlite::{connect_sqlite, run_migrations},
    transport::http,
};
use serde_json::{Value, json};
use tower::ServiceExt;

#[path = "support/generic_client.rs"]
mod generic_client;

use generic_client::GenericClientTestScope;

const TOKEN: &str = "test-token";

async fn test_state(roots: Vec<WorkspaceRootConfig>) -> AppState {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("workspace_api.db");
    let _kept_dir = dir.keep();
    let database_url = format!("sqlite://{}", db_path.display());
    let db = connect_sqlite(&database_url).await.expect("connect");
    run_migrations(&db).await.expect("migrate");
    AppState {
        db,
        external_api_token: Some(TOKEN.to_string()),
        graph: GraphRuntimeConfig::default(),
        workspace_browser: WorkspaceBrowserConfig { roots },
        dashboard: llmparty::transport::http::dashboard::ResolvedDashboard::local_default(),
    }
}

async fn get_json(state: AppState, uri: &str) -> (StatusCode, Value) {
    let response = http::router(state)
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(uri)
                .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    json_response(response).await
}

async fn post_json(state: AppState, uri: &str, body: Value) -> (StatusCode, Value) {
    let response = http::router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
                .body(Body::from(body.to_string()))
                .expect("request"),
        )
        .await
        .expect("response");
    json_response(response).await
}

async fn delete_json(state: AppState, uri: &str) -> (StatusCode, Value) {
    let response = http::router(state)
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(uri)
                .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    json_response(response).await
}

async fn patch_json(state: AppState, uri: &str, body: Value) -> (StatusCode, Value) {
    let response = http::router(state)
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(uri)
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
                .body(Body::from(body.to_string()))
                .expect("request"),
        )
        .await
        .expect("response");
    json_response(response).await
}

async fn json_response(response: axum::response::Response) -> (StatusCode, Value) {
    let status = response.status();
    let body = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    let json = serde_json::from_slice(&body).expect("json body");
    (status, json)
}

#[tokio::test]
async fn lists_configured_workspace_roots_without_persisting_them() {
    let root = tempfile::tempdir().expect("root");
    let canonical = std::fs::canonicalize(root.path()).expect("canonical");
    let state = test_state(vec![WorkspaceRootConfig {
        root_id: "projects".to_string(),
        label: "Projects".to_string(),
        path: root.path().display().to_string(),
    }])
    .await;

    let (status, body) = get_json(state, "/external/v1/workspace-roots").await;

    assert_eq!(status, StatusCode::OK);
    let roots = body["data"]["roots"].as_array().expect("roots");
    assert_eq!(roots.len(), 1);
    assert_eq!(roots[0]["root_id"], "projects");
    assert_eq!(roots[0]["label"], "Projects");
    assert_eq!(roots[0]["canonical_path"], canonical.display().to_string());
    assert_eq!(roots[0]["state"], "available");
}

#[tokio::test]
async fn browses_only_directories_inside_configured_root() {
    let root = tempfile::tempdir().expect("root");
    std::fs::create_dir(root.path().join("app")).expect("app");
    std::fs::write(root.path().join("README.md"), "ignored").expect("file");
    std::fs::create_dir(root.path().join("node_modules")).expect("ignored dir");
    let state = test_state(vec![WorkspaceRootConfig {
        root_id: "projects".to_string(),
        label: "Projects".to_string(),
        path: root.path().display().to_string(),
    }])
    .await;

    let (status, body) = get_json(state, "/external/v1/workspace-roots/projects/entries").await;

    assert_eq!(status, StatusCode::OK);
    let entries = body["data"]["entries"].as_array().expect("entries");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["name"], "app");
    assert_eq!(entries[0]["path"], "app");
    assert_eq!(entries[0]["kind"], "directory");
    assert_eq!(entries[0]["is_workspace"], false);
}

#[tokio::test]
async fn rejects_directory_browsing_that_escapes_root() {
    let root = tempfile::tempdir().expect("root");
    let state = test_state(vec![WorkspaceRootConfig {
        root_id: "projects".to_string(),
        label: "Projects".to_string(),
        path: root.path().display().to_string(),
    }])
    .await;

    let (status, body) = get_json(
        state,
        "/external/v1/workspace-roots/projects/entries?path=..",
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "invalid_request");
}

#[tokio::test]
async fn registers_existing_directory_under_allowed_root_without_storing_root_id() {
    let root = tempfile::tempdir().expect("root");
    let app = root.path().join("app");
    std::fs::create_dir(&app).expect("app");
    let canonical = std::fs::canonicalize(&app).expect("canonical");
    let state = test_state(vec![WorkspaceRootConfig {
        root_id: "projects".to_string(),
        label: "Projects".to_string(),
        path: root.path().display().to_string(),
    }])
    .await;

    let (status, body) = post_json(
        state.clone(),
        "/external/v1/workspaces",
        json!({"root_id":"projects", "path":"app", "name":"App"}),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    let workspace = &body["data"]["workspace"];
    assert_eq!(workspace["canonical_path"], canonical.display().to_string());
    assert_eq!(workspace["name"], "App");
    assert!(workspace.get("root_id").is_none());

    let workspace_id = workspace["workspace_id"].as_str().expect("workspace id");
    let (status, body) = get_json(state, &format!("/external/v1/workspaces/{workspace_id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["workspace"]["workspace_id"], workspace_id);
    assert!(body["data"]["workspace"].get("root_id").is_none());
}

#[tokio::test]
async fn renames_workspace_without_changing_path() {
    let root = tempfile::tempdir().expect("root");
    let app = root.path().join("app");
    std::fs::create_dir(&app).expect("app");
    let canonical = std::fs::canonicalize(&app).expect("canonical");
    let state = test_state(vec![WorkspaceRootConfig {
        root_id: "projects".to_string(),
        label: "Projects".to_string(),
        path: root.path().display().to_string(),
    }])
    .await;
    let (_, body) = post_json(
        state.clone(),
        "/external/v1/workspaces",
        json!({"root_id":"projects", "path":"app", "name":"App"}),
    )
    .await;
    let workspace_id = body["data"]["workspace"]["workspace_id"].as_str().unwrap();

    let (status, body) = patch_json(
        state.clone(),
        &format!("/external/v1/workspaces/{workspace_id}"),
        json!({"name":"Renamed App"}),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let workspace = &body["data"]["workspace"];
    assert_eq!(workspace["workspace_id"], workspace_id);
    assert_eq!(workspace["name"], "Renamed App");
    assert_eq!(workspace["canonical_path"], canonical.display().to_string());

    let (status, body) = patch_json(
        state,
        &format!("/external/v1/workspaces/{workspace_id}"),
        json!({"name":"   "}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["data"]["workspace"]["name"].is_null());
}

#[tokio::test]
async fn soft_deletes_workspace_hiding_it_from_list_but_preserving_direct_lookup() {
    let root = tempfile::tempdir().expect("root");
    let app = root.path().join("app");
    std::fs::create_dir(&app).expect("app");
    let state = test_state(vec![WorkspaceRootConfig {
        root_id: "projects".to_string(),
        label: "Projects".to_string(),
        path: root.path().display().to_string(),
    }])
    .await;
    let (_, body) = post_json(
        state.clone(),
        "/external/v1/workspaces",
        json!({"root_id":"projects", "path":"app"}),
    )
    .await;
    let workspace_id = body["data"]["workspace"]["workspace_id"].as_str().unwrap();

    let (status, body) = delete_json(
        state.clone(),
        &format!("/external/v1/workspaces/{workspace_id}"),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["workspace"]["workspace_id"], workspace_id);
    assert_eq!(body["data"]["workspace"]["state"], "deleted");

    let (status, body) = get_json(state.clone(), "/external/v1/workspaces").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["data"]["workspaces"].as_array().unwrap().is_empty());

    let (status, body) = get_json(
        state.clone(),
        &format!("/external/v1/workspaces/{workspace_id}"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["workspace"]["state"], "deleted");

    let (status, body) =
        delete_json(state, &format!("/external/v1/workspaces/{workspace_id}")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["workspace"]["state"], "deleted");
}

#[tokio::test]
async fn does_not_register_missing_directory() {
    let root = tempfile::tempdir().expect("root");
    let state = test_state(vec![WorkspaceRootConfig {
        root_id: "projects".to_string(),
        label: "Projects".to_string(),
        path: root.path().display().to_string(),
    }])
    .await;

    let (status, body) = post_json(
        state,
        "/external/v1/workspaces",
        json!({"root_id":"projects", "path":"missing"}),
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "not_found");
}

#[tokio::test]
async fn creates_session_from_known_workspace_id() {
    let _scope = GenericClientTestScope::new().await;
    let root = tempfile::tempdir().expect("root");
    let app = root.path().join("app");
    std::fs::create_dir(&app).expect("app");
    let canonical = std::fs::canonicalize(&app).expect("canonical");
    let state = test_state(vec![WorkspaceRootConfig {
        root_id: "projects".to_string(),
        label: "Projects".to_string(),
        path: root.path().display().to_string(),
    }])
    .await;
    let (_, body) = post_json(
        state.clone(),
        "/external/v1/workspaces",
        json!({"root_id":"projects", "path":"app"}),
    )
    .await;
    let workspace_id = body["data"]["workspace"]["workspace_id"].as_str().unwrap();

    let (status, body) = post_json(
        state,
        "/external/v1/sessions",
        json!({"client_type":"generic", "workspace_id": workspace_id}),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    assert!(body["data"]["session"]["session_id"].as_str().is_some());
    assert_eq!(body["data"]["session"]["workspace_id"], workspace_id);
    assert_eq!(
        body["data"]["session"]["workspace"],
        canonical.display().to_string()
    );
}
