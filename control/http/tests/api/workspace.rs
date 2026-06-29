use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use pontia_application::{AppState, FilePickerConfig, WorkspaceBrowserConfig, WorkspaceRootConfig};
use pontia_config::GraphRuntimeConfig;
use pontia_http as http;
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::{generic_client::GenericClientTestScope, test_app::TestApp};

const TOKEN: &str = "test-token";

async fn test_state(roots: Vec<WorkspaceRootConfig>) -> AppState {
    test_state_with_file_picker(roots, FilePickerConfig::default()).await
}

async fn test_state_with_file_picker(
    roots: Vec<WorkspaceRootConfig>,
    file_picker: FilePickerConfig,
) -> AppState {
    TestApp::builder()
        .database_name("workspace_api.db")
        .external_api_token(Some(TOKEN.to_string()))
        .graph(GraphRuntimeConfig::default())
        .workspace_browser(WorkspaceBrowserConfig { roots })
        .file_picker(file_picker)
        .build_state()
        .await
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

async fn post_empty(state: AppState, uri: &str) -> (StatusCode, Value) {
    let response = http::router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    json_response(response).await
}

fn git(workdir: &std::path::Path, args: &[&str]) {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(workdir)
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {:?} failed: {}{}",
        args,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
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
async fn git_status_read_returns_unknown_until_workspace_is_observed() {
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

    let (status, body) = get_json(
        state,
        &format!("/external/v1/workspaces/{workspace_id}/git-status"),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let git_status = &body["data"]["git_status"];
    assert_eq!(git_status["workspace_id"], workspace_id);
    assert_eq!(git_status["state"], "unknown");
    assert!(git_status["observed_at"].is_null());
}

#[tokio::test]
async fn refreshing_git_status_updates_sqlite_projection_read_by_get() {
    let root = tempfile::tempdir().expect("root");
    let app = root.path().join("app");
    std::fs::create_dir(&app).expect("app");
    git(&app, &["init", "-b", "main"]);
    std::fs::write(app.join("README.md"), "hello\n").expect("tracked file");
    git(&app, &["add", "README.md"]);
    git(
        &app,
        &[
            "-c",
            "user.email=test@example.com",
            "-c",
            "user.name=Test",
            "commit",
            "-m",
            "init",
        ],
    );
    std::fs::write(app.join("README.md"), "hello changed\n").expect("modify tracked file");
    std::fs::write(app.join("notes.txt"), "untracked\n").expect("untracked file");
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

    let (status, body) = post_empty(
        state.clone(),
        &format!("/external/v1/workspaces/{workspace_id}/git-status/refresh"),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let refreshed = &body["data"]["git_status"];
    assert_eq!(refreshed["state"], "observed");
    assert_eq!(refreshed["branch"], "main");
    assert_eq!(refreshed["clean"], false);
    assert_eq!(refreshed["unstaged_count"], 1);
    assert_eq!(refreshed["untracked_count"], 1);
    assert!(refreshed["observed_at"].as_str().is_some());

    let (status, body) = get_json(
        state,
        &format!("/external/v1/workspaces/{workspace_id}/git-status"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["git_status"], *refreshed);
}

#[tokio::test]
async fn file_picker_returns_directories_and_files_and_respects_ignore_config() {
    let root = tempfile::tempdir().expect("root");
    let app = root.path().join("app");
    std::fs::create_dir_all(app.join("src")).expect("src");
    std::fs::create_dir_all(app.join("node_modules/pkg")).expect("node_modules");
    std::fs::write(app.join("src/main.rs"), "fn main() {}\n").expect("main");
    std::fs::write(app.join("README.md"), "hello\n").expect("readme");
    std::fs::write(
        app.join("node_modules/pkg/index.js"),
        "module.exports = {}\n",
    )
    .expect("dep");
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

    let (status, body) = get_json(
        state,
        &format!("/external/v1/workspaces/{workspace_id}/file-picker?query=src"),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let files = body["data"]["files"].as_array().expect("files");
    assert!(
        files.iter().any(|entry| entry["path"] == "src"
            && entry["name"] == "src"
            && entry["kind"] == "directory"),
        "files: {files:?}"
    );
    assert!(
        files.iter().any(|entry| entry["path"] == "src/main.rs"
            && entry["name"] == "main.rs"
            && entry["kind"] == "file"),
        "files: {files:?}"
    );
    assert!(
        files
            .iter()
            .all(|file| file["path"] != "node_modules/pkg/index.js"
                && file["path"] != "node_modules/pkg")
    );
}

#[tokio::test]
async fn file_picker_can_include_hidden_and_normally_ignored_files_from_config() {
    let root = tempfile::tempdir().expect("root");
    let app = root.path().join("app");
    std::fs::create_dir_all(app.join(".git/refs")).expect("git");
    std::fs::create_dir_all(app.join("node_modules/pkg")).expect("node_modules");
    std::fs::write(app.join(".git/HEAD"), "ref: refs/heads/main\n").expect("head");
    std::fs::write(
        app.join("node_modules/pkg/index.js"),
        "module.exports = {}\n",
    )
    .expect("dep");
    let file_picker = FilePickerConfig {
        include_hidden: true,
        respect_gitignore: false,
        respect_ignore_files: false,
        respect_git_exclude: false,
        ignore_globs: vec![],
        ..FilePickerConfig::default()
    };
    let state = test_state_with_file_picker(
        vec![WorkspaceRootConfig {
            root_id: "projects".to_string(),
            label: "Projects".to_string(),
            path: root.path().display().to_string(),
        }],
        file_picker,
    )
    .await;
    let (_, body) = post_json(
        state.clone(),
        "/external/v1/workspaces",
        json!({"root_id":"projects", "path":"app"}),
    )
    .await;
    let workspace_id = body["data"]["workspace"]["workspace_id"].as_str().unwrap();

    let (status, body) = get_json(
        state,
        &format!("/external/v1/workspaces/{workspace_id}/file-picker?query=head"),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let paths: Vec<_> = body["data"]["files"]
        .as_array()
        .expect("files")
        .iter()
        .map(|file| file["path"].as_str().unwrap().to_string())
        .collect();
    assert!(paths.contains(&".git/HEAD".to_string()), "paths: {paths:?}");
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
async fn list_workspaces_returns_only_active_workspaces() {
    let root = tempfile::tempdir().expect("root");
    let active_app = root.path().join("active-app");
    let archived_app = root.path().join("archived-app");
    std::fs::create_dir(&active_app).expect("active app");
    std::fs::create_dir(&archived_app).expect("archived app");
    let state = test_state(vec![WorkspaceRootConfig {
        root_id: "projects".to_string(),
        label: "Projects".to_string(),
        path: root.path().display().to_string(),
    }])
    .await;

    let (_, active_body) = post_json(
        state.clone(),
        "/external/v1/workspaces",
        json!({"root_id":"projects", "path":"active-app"}),
    )
    .await;
    let active_workspace_id = active_body["data"]["workspace"]["workspace_id"]
        .as_str()
        .unwrap();
    let (_, archived_body) = post_json(
        state.clone(),
        "/external/v1/workspaces",
        json!({"root_id":"projects", "path":"archived-app"}),
    )
    .await;
    let archived_workspace_id = archived_body["data"]["workspace"]["workspace_id"]
        .as_str()
        .unwrap();

    sqlx::query("UPDATE workspaces SET state = 'archived' WHERE workspace_id = ?")
        .bind(archived_workspace_id)
        .execute(&state.db())
        .await
        .expect("archive workspace");

    let (status, body) = get_json(state.clone(), "/external/v1/workspaces").await;
    assert_eq!(status, StatusCode::OK);
    let workspaces = body["data"]["workspaces"].as_array().unwrap();
    assert_eq!(workspaces.len(), 1);
    assert_eq!(workspaces[0]["workspace_id"], active_workspace_id);
    assert_eq!(workspaces[0]["state"], "active");

    let (status, body) = get_json(
        state,
        &format!("/external/v1/workspaces/{archived_workspace_id}"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["workspace"]["state"], "archived");
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
