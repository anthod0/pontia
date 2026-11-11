use std::{fs, path::Path};

use super::*;

fn root_config(id: &str, path: &Path) -> WorkspaceRootConfig {
    WorkspaceRootConfig {
        root_id: id.into(),
        label: id.into(),
        path: path.display().to_string(),
    }
}

async fn register(state: &AppState, path: &str) -> Value {
    let (status, body) = post_json(
        state.clone(),
        "/api/v1/workspaces",
        json!({"root_id": "projects", "path": path}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    body["data"]["workspace"].clone()
}

async fn listed_workspace(state: &AppState, id: &str) -> Value {
    let (status, body) = get_json(state.clone(), "/api/v1/workspaces").await;
    assert_eq!(status, StatusCode::OK);
    body["data"]["workspaces"]
        .as_array()
        .unwrap()
        .iter()
        .find(|workspace| workspace["workspace_id"] == id)
        .unwrap()
        .clone()
}

#[tokio::test]
async fn availability_is_consistent_across_workspace_responses() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("app")).unwrap();
    let state = test_state(vec![root_config("projects", root.path())]).await;
    let workspace = register(&state, "app").await;
    let id = workspace["workspace_id"].as_str().unwrap();
    let available = json!({"status": "available", "root_id": "projects", "path": "app"});
    assert_eq!(workspace["availability"], available);
    assert_eq!(
        listed_workspace(&state, id).await["availability"],
        available
    );
    let uri = format!("/api/v1/workspaces/{id}");
    let (status, body) = get_json(state.clone(), &uri).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["workspace"]["availability"], available);
    let (status, body) = patch_json(state.clone(), &uri, json!({"name": "Renamed"})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["workspace"]["availability"], available);
    let (status, body) = delete_json(state, &uri).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["workspace"]["availability"], available);
    assert_eq!(body["data"]["workspace"]["state"], "deleted");
}

#[tokio::test]
async fn refresh_distinguishes_missing_directory_file_and_restored_directory() {
    let root = tempfile::tempdir().unwrap();
    let app = root.path().join("app");
    fs::create_dir(&app).unwrap();
    let state = test_state(vec![root_config("projects", root.path())]).await;
    let workspace = register(&state, "app").await;
    let id = workspace["workspace_id"].as_str().unwrap();
    fs::remove_dir(&app).unwrap();
    let missing = listed_workspace(&state, id).await;
    assert_eq!(
        missing["availability"],
        json!({"status": "unavailable", "reason": "missing_directory"})
    );
    assert_eq!(missing["state"], "active");
    fs::write(&app, "file").unwrap();
    assert_eq!(
        listed_workspace(&state, id).await["availability"],
        json!({"status": "unavailable", "reason": "not_directory"})
    );
    fs::remove_file(&app).unwrap();
    fs::create_dir(&app).unwrap();
    assert_eq!(
        listed_workspace(&state, id).await["availability"],
        workspace["availability"]
    );
}

#[tokio::test]
async fn unavailable_root_does_not_mean_outside_root() {
    let root = tempfile::tempdir().unwrap();
    let projects = root.path().join("projects");
    let moved = root.path().join("moved");
    fs::create_dir(&projects).unwrap();
    let state = test_state(vec![root_config("projects", &projects)]).await;
    let workspace = register(&state, "").await;
    let id = workspace["workspace_id"].as_str().unwrap();
    fs::rename(&projects, &moved).unwrap();
    assert_eq!(
        listed_workspace(&state, id).await["availability"],
        json!({"status": "unknown", "reason": "root_unavailable"})
    );
    fs::rename(&moved, &projects).unwrap();
    assert_eq!(
        listed_workspace(&state, id).await["availability"],
        workspace["availability"]
    );
}

#[tokio::test]
async fn uses_deepest_root_and_does_not_match_path_prefix_siblings() {
    let root = tempfile::tempdir().unwrap();
    let projects = root.path().join("projects");
    let app = projects.join("app");
    let sibling = root.path().join("projects-other");
    fs::create_dir_all(&app).unwrap();
    fs::create_dir(&sibling).unwrap();
    let state = test_state(vec![
        root_config("projects", &projects),
        root_config("nested", &app),
    ])
    .await;
    let workspace = register(&state, "app").await;
    assert_eq!(
        workspace["availability"],
        json!({"status": "available", "root_id": "nested", "path": ""})
    );
    let outside =
        pontia_agent_control::upsert_workspace(&state.db(), &sibling.display().to_string())
            .await
            .unwrap();
    assert_eq!(
        listed_workspace(&state, &outside.workspace_id).await["availability"],
        json!({"status": "unavailable", "reason": "outside_root"})
    );
}

#[tokio::test]
async fn filesystem_root_supports_workspace_navigation() {
    let root = tempfile::tempdir().unwrap();
    let path = fs::canonicalize(root.path()).unwrap();
    let state = test_state(vec![root_config("projects", Path::new("/"))]).await;
    let workspace = register(&state, path.strip_prefix("/").unwrap().to_str().unwrap()).await;
    assert_eq!(
        workspace["availability"],
        json!({"status": "available", "root_id": "projects", "path": path.strip_prefix("/").unwrap().to_str().unwrap()})
    );
}

#[cfg(unix)]
#[tokio::test]
async fn symlink_escape_and_resolution_failure_are_not_available() {
    use std::os::unix::fs::symlink;
    let root = tempfile::tempdir().unwrap();
    let projects = root.path().join("projects");
    let app = projects.join("app");
    let outside = root.path().join("outside");
    fs::create_dir_all(&app).unwrap();
    fs::create_dir(&outside).unwrap();
    let state = test_state(vec![root_config("projects", &projects)]).await;
    let workspace = register(&state, "app").await;
    let id = workspace["workspace_id"].as_str().unwrap();
    fs::remove_dir(&app).unwrap();
    symlink(&outside, &app).unwrap();
    assert_eq!(
        listed_workspace(&state, id).await["availability"],
        json!({"status": "unavailable", "reason": "outside_root"})
    );
    fs::remove_file(&app).unwrap();
    symlink(&app, &app).unwrap();
    assert_eq!(
        listed_workspace(&state, id).await["availability"],
        json!({"status": "unknown", "reason": "check_failed"})
    );
}

#[cfg(unix)]
#[tokio::test]
async fn unreadable_directory_reports_permission_denied_without_hiding_other_workspaces() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let app = root.path().join("app");
    fs::create_dir(&app).unwrap();
    let state = test_state(vec![root_config("projects", root.path())]).await;
    let workspace = register(&state, "app").await;
    let readable = register(&state, "").await;
    fs::set_permissions(&app, fs::Permissions::from_mode(0o000)).unwrap();
    let denied = fs::read_dir(&app).is_err();
    let (status, body) = get_json(state, "/api/v1/workspaces").await;
    fs::set_permissions(&app, fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(status, StatusCode::OK);
    let workspaces = body["data"]["workspaces"].as_array().unwrap();
    assert_eq!(workspaces.len(), 2);
    let restricted = workspaces
        .iter()
        .find(|item| item["workspace_id"] == workspace["workspace_id"])
        .unwrap();
    if denied {
        assert_eq!(
            restricted["availability"],
            json!({"status": "unavailable", "reason": "permission_denied"})
        );
    } else {
        assert_eq!(restricted["availability"], workspace["availability"]);
    }
    let accessible = workspaces
        .iter()
        .find(|item| item["workspace_id"] == readable["workspace_id"])
        .unwrap();
    assert_eq!(accessible["availability"], readable["availability"]);
}

#[tokio::test]
async fn directory_registration_markers_follow_activation_and_deactivation() {
    let root = tempfile::tempdir().unwrap();
    for name in ["app", "other", "inactive"] {
        fs::create_dir(root.path().join(name)).unwrap();
    }
    let state = test_state(vec![root_config("projects", root.path())]).await;
    register(&state, "app").await;
    let inactive = register(&state, "inactive").await;
    let uri = format!(
        "/api/v1/workspaces/{}",
        inactive["workspace_id"].as_str().unwrap()
    );
    let (status, _) = delete_json(state.clone(), &uri).await;
    assert_eq!(status, StatusCode::OK);
    let (status, body) = get_json(state, "/api/v1/workspace-roots/projects/entries").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["data"]["entries"],
        json!([
            {"name": "app", "path": "app", "kind": "directory", "is_workspace": true},
            {"name": "inactive", "path": "inactive", "kind": "directory", "is_workspace": false},
            {"name": "other", "path": "other", "kind": "directory", "is_workspace": false}
        ])
    );
}
