use std::{fs, path::Path};

use super::*;

const COMMIT: &[&str] = &[
    "-c",
    "user.email=test@example.com",
    "-c",
    "user.name=Test",
    "commit",
    "-qm",
];

async fn workspace_for(root: &Path, path: &str) -> (AppState, String, tempfile::TempDir) {
    let (state, pontia_home) = test_state(vec![WorkspaceRootConfig {
        root_id: "projects".to_string(),
        label: "Projects".to_string(),
        path: root.display().to_string(),
    }])
    .await;
    let (status, body) = post_json(
        state.clone(),
        "/api/v1/workspaces",
        json!({"root_id": "projects", "path": path}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let id = body["data"]["workspace"]["workspace_id"]
        .as_str()
        .unwrap()
        .to_string();
    (state, id, pontia_home)
}

fn repo_with_changes(app: &Path) {
    fs::create_dir_all(app.join("src")).unwrap();
    git(app, &["init", "-q", "-b", "main"]);
    fs::write(app.join("README.md"), "one\ntwo\nthree\n").unwrap();
    fs::write(app.join("src/lib.rs"), "fn a() {}\n").unwrap();
    fs::write(app.join("old.txt"), "keep\n").unwrap();
    fs::write(app.join("logo.bin"), b"\0\x01").unwrap();
    git(app, &["add", "."]);
    git(app, &[COMMIT, &["init"]].concat());
    // Staged change, then a further unstaged change to the same file.
    fs::write(app.join("README.md"), "one\nTWO\nthree\n").unwrap();
    git(app, &["add", "README.md"]);
    fs::write(app.join("README.md"), "one\nTWO\nthree\nfour\n").unwrap();
    fs::remove_file(app.join("src/lib.rs")).unwrap();
    git(app, &["mv", "old.txt", "new name.txt"]);
    fs::write(app.join("logo.bin"), b"\0\x02").unwrap();
    fs::write(app.join("notes.txt"), "a\nb").unwrap();
    fs::create_dir(app.join("scratch")).unwrap();
    fs::write(app.join("scratch/x"), "x\n").unwrap();
}

#[tokio::test]
async fn git_changes_lists_grouped_files_with_line_counts() {
    let root = tempfile::tempdir().unwrap();
    let app = root.path().join("app");
    repo_with_changes(&app);
    let (state, id, _pontia_home) = workspace_for(root.path(), "app").await;

    let (status, body) = get_json(state, &format!("/api/v1/workspaces/{id}/git/changes")).await;

    assert_eq!(status, StatusCode::OK, "{body}");
    let changes = &body["data"]["changes"];
    assert_eq!(changes["truncated"], false);
    let files = changes["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|file| {
            (
                file["group"].as_str().unwrap().to_string(),
                file["status"].as_str().unwrap().to_string(),
                file["path"].as_str().unwrap().to_string(),
                file["additions"].clone(),
                file["deletions"].clone(),
                file["binary"].as_bool().unwrap(),
            )
        })
        .collect::<Vec<_>>();
    let expect = |group: &str, status: &str, path: &str, add: Value, del: Value, bin: bool| {
        (
            group.to_string(),
            status.to_string(),
            path.to_string(),
            add,
            del,
            bin,
        )
    };
    assert_eq!(
        files,
        vec![
            expect("staged", "M", "README.md", json!(1), json!(1), false),
            expect("staged", "R", "new name.txt", json!(0), json!(0), false),
            expect("unstaged", "M", "README.md", json!(1), json!(0), false),
            expect("unstaged", "M", "logo.bin", Value::Null, Value::Null, true),
            expect("unstaged", "D", "src/lib.rs", json!(0), json!(1), false),
            expect("untracked", "?", "notes.txt", json!(2), json!(0), false),
            expect(
                "untracked",
                "?",
                "scratch/",
                Value::Null,
                Value::Null,
                false
            ),
        ]
    );
    assert_eq!(changes["files"][1]["orig_path"], "old.txt");
    assert_eq!(changes["total"], 7);
}

#[tokio::test]
async fn git_changes_reports_non_git_workspace() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("notes")).unwrap();
    let (state, id, _pontia_home) = workspace_for(root.path(), "notes").await;

    let (status, body) = get_json(state, &format!("/api/v1/workspaces/{id}/git/changes")).await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body["data"]["changes"]["repo_root"].is_null());
    assert_eq!(body["data"]["changes"]["files"], json!([]));
}

#[tokio::test]
async fn git_diff_returns_hunks_for_each_group() {
    let root = tempfile::tempdir().unwrap();
    let app = root.path().join("app");
    repo_with_changes(&app);
    let (state, id, _pontia_home) = workspace_for(root.path(), "app").await;
    let diff = |query: &str| {
        let state = state.clone();
        let uri = format!("/api/v1/workspaces/{id}/git/diff?{query}");
        async move { get_json(state, &uri).await }
    };

    let (status, body) = diff("path=README.md&group=staged").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let lines = &body["data"]["diff"]["hunks"][0]["lines"];
    assert_eq!(
        lines[1],
        json!({"kind": "delete", "old": 2, "new": null, "text": "two"})
    );
    assert_eq!(
        lines[2],
        json!({"kind": "add", "old": null, "new": 2, "text": "TWO"})
    );

    let (_, body) = diff("path=README.md&group=unstaged").await;
    assert_eq!(body["data"]["diff"]["hunks"][0]["lines"][3]["text"], "four");

    let (_, body) = diff("path=notes.txt&group=untracked").await;
    let hunk = &body["data"]["diff"]["hunks"][0];
    assert_eq!(
        hunk["lines"][1],
        json!({"kind": "add", "old": null, "new": 2, "text": "b"})
    );

    let (_, body) = diff("path=logo.bin&group=unstaged").await;
    assert_eq!(body["data"]["diff"]["binary"], true);

    let (_, body) = diff("path=new%20name.txt&orig_path=old.txt&group=staged").await;
    assert_eq!(body["data"]["diff"]["hunks"], json!([]));
    assert_eq!(body["data"]["diff"]["binary"], false);
}

#[tokio::test]
async fn git_diff_rejects_paths_outside_the_repository() {
    let root = tempfile::tempdir().unwrap();
    let app = root.path().join("app");
    repo_with_changes(&app);
    fs::write(root.path().join("secret.txt"), "secret\n").unwrap();
    std::os::unix::fs::symlink(root.path().join("secret.txt"), app.join("link")).unwrap();
    let (state, id, _pontia_home) = workspace_for(root.path(), "app").await;

    for query in [
        "path=../secret.txt&group=untracked",
        "path=%2Fetc%2Fpasswd&group=untracked",
        "path=README.md&orig_path=../secret.txt&group=staged",
    ] {
        let (status, body) = get_json(
            state.clone(),
            &format!("/api/v1/workspaces/{id}/git/diff?{query}"),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{query}: {body}");
    }

    let response = http::router(state.clone())
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/v1/workspaces/{id}/git/diff?path=README.md&group=everything"
                ))
                .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    // A tracked path is not read as an untracked file.
    let (status, _) = get_json(
        state.clone(),
        &format!("/api/v1/workspaces/{id}/git/diff?path=README.md&group=untracked"),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // An untracked symbolic link shows its target path, never the target's content.
    let (status, body) = get_json(
        state,
        &format!("/api/v1/workspaces/{id}/git/diff?path=link&group=untracked"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let text = body["data"]["diff"]["hunks"][0]["lines"][0]["text"]
        .as_str()
        .unwrap();
    assert!(text.ends_with("secret.txt"), "{text}");
    assert!(!body.to_string().contains("secret\\n"));
}
