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

/// `main` has three commits; `feature` branches off after the first and adds one.
fn repo_with_history(app: &Path) {
    fs::create_dir_all(app).unwrap();
    git(app, &["init", "-q", "-b", "main"]);
    fs::write(app.join("README.md"), "one\n").unwrap();
    fs::write(app.join("old.txt"), "keep\n").unwrap();
    git(app, &["add", "."]);
    git(app, &[COMMIT, &["initial"]].concat());
    git(app, &["branch", "feature"]);
    fs::write(app.join("README.md"), "one\ntwo\n").unwrap();
    git(app, &["mv", "old.txt", "new name.txt"]);
    git(app, &["add", "."]);
    git(
        app,
        &[COMMIT, &["update readme\n\nAdds a line.\nAnd renames."]].concat(),
    );
    fs::write(app.join("logo.bin"), b"\0\x01").unwrap();
    git(app, &["add", "."]);
    git(app, &[COMMIT, &["add logo"]].concat());
    git(app, &["switch", "-q", "feature"]);
    fs::write(app.join("feature.txt"), "f\n").unwrap();
    git(app, &["add", "."]);
    git(app, &[COMMIT, &["feature work"]].concat());
    git(app, &["switch", "-q", "main"]);
    // Uncommitted work is never touched by reading history.
    fs::write(app.join("README.md"), "dirty\n").unwrap();
}

fn subjects(body: &Value) -> Vec<&str> {
    body["data"]["commits"]["commits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|commit| commit["subject"].as_str().unwrap())
        .collect()
}

#[tokio::test]
async fn git_history_reads_branches_commits_and_commit_diffs() {
    let root = tempfile::tempdir().unwrap();
    let app = root.path().join("app");
    repo_with_history(&app);
    let (state, id, _pontia_home) = workspace_for(root.path(), "app").await;
    let get = |uri: String| {
        let state = state.clone();
        async move { get_json(state, &uri).await }
    };
    let base = format!("/api/v1/workspaces/{id}/git");

    let (status, body) = get(format!("{base}/branches")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let branches = &body["data"]["branches"];
    assert_eq!(branches["head"], "refs/heads/main");
    let mut names = branches["branches"]
        .as_array()
        .unwrap()
        .iter()
        .map(|branch| {
            (
                branch["name"].as_str().unwrap(),
                branch["ref"].as_str().unwrap(),
                branch["current"].as_bool().unwrap(),
            )
        })
        .collect::<Vec<_>>();
    names.sort();
    assert_eq!(
        names,
        vec![
            ("feature", "refs/heads/feature", false),
            ("main", "refs/heads/main", true),
        ]
    );

    // Reading another branch's history leaves the checkout where it is.
    let (status, body) = get(format!("{base}/commits?ref=refs%2Fheads%2Ffeature")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(subjects(&body), vec!["feature work", "initial"]);
    assert_eq!(body["data"]["commits"]["has_more"], false);
    assert_eq!(
        fs::read_to_string(app.join("README.md")).unwrap(),
        "dirty\n"
    );
    assert!(!app.join("feature.txt").exists());

    let (_, body) = get(format!("{base}/commits?ref=refs%2Fheads%2Fmain")).await;
    assert_eq!(
        subjects(&body),
        vec!["add logo", "update readme", "initial"]
    );
    let (_, body) = get(format!("{base}/commits?ref=HEAD&skip=2")).await;
    assert_eq!(subjects(&body), vec!["initial"]);
    let initial = body["data"]["commits"]["commits"][0]["sha"]
        .as_str()
        .unwrap()
        .to_string();
    let (_, body) = get(format!("{base}/commits?ref=refs%2Fheads%2Fmain&skip=1")).await;
    let update = &body["data"]["commits"]["commits"][0];
    assert_eq!(update["parents"], json!([initial]));
    let update = update["sha"].as_str().unwrap().to_string();

    let (status, body) = get(format!("{base}/commits/{update}")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let commit = &body["data"]["commit"];
    assert_eq!(commit["subject"], "update readme");
    assert_eq!(commit["body"], "Adds a line.\nAnd renames.");
    assert_eq!(commit["author_name"], "Test");
    assert_eq!(commit["total"], 2);
    assert_eq!(
        commit["files"],
        json!([
            {"path": "README.md", "status": "M", "additions": 1, "deletions": 0, "binary": false},
            {"path": "new name.txt", "orig_path": "old.txt", "status": "R", "additions": 0, "deletions": 0, "binary": false},
        ])
    );

    // A root commit lists every file it adds.
    let (_, body) = get(format!("{base}/commits/{initial}")).await;
    let statuses = body["data"]["commit"]["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|file| file["status"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(statuses, vec!["A", "A"]);

    let (status, body) = get(format!("{base}/commits/{update}/diff?path=README.md")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["data"]["diff"]["hunks"][0]["lines"][1],
        json!({"kind": "add", "old": null, "new": 2, "text": "two"})
    );
    let (_, body) = get(format!("{base}/commits/{initial}/diff?path=old.txt")).await;
    assert_eq!(body["data"]["diff"]["hunks"][0]["lines"][0]["text"], "keep");
    let (_, body) = get(format!(
        "{base}/commits/{update}/diff?path=new%20name.txt&orig_path=old.txt"
    ))
    .await;
    assert_eq!(body["data"]["diff"]["hunks"], json!([]));
    let (_, body) = get(format!("{base}/commits?ref=HEAD")).await;
    let logo = body["data"]["commits"]["commits"][0]["sha"]
        .as_str()
        .unwrap()
        .to_string();
    let (_, body) = get(format!("{base}/commits/{logo}/diff?path=logo.bin")).await;
    assert_eq!(body["data"]["diff"]["binary"], true);
}

#[tokio::test]
async fn git_history_rejects_invalid_refs_and_commits() {
    let root = tempfile::tempdir().unwrap();
    let app = root.path().join("app");
    repo_with_history(&app);
    let (state, id, _pontia_home) = workspace_for(root.path(), "app").await;
    let base = format!("/api/v1/workspaces/{id}/git");
    let missing = "0".repeat(40);

    for (uri, expected) in [
        (format!("{base}/commits?ref=main"), StatusCode::BAD_REQUEST),
        (format!("{base}/commits?ref=--all"), StatusCode::BAD_REQUEST),
        (
            format!("{base}/commits?ref=refs%2Fheads%2Fmissing"),
            StatusCode::NOT_FOUND,
        ),
        (format!("{base}/commits/HEAD"), StatusCode::BAD_REQUEST),
        (format!("{base}/commits/{missing}"), StatusCode::NOT_FOUND),
        (
            format!("{base}/commits/{missing}/diff?path=README.md"),
            StatusCode::NOT_FOUND,
        ),
        (
            format!("{base}/commits/{missing}/diff?path=..%2Fsecret"),
            StatusCode::BAD_REQUEST,
        ),
    ] {
        let (status, body) = get_json(state.clone(), &uri).await;
        assert_eq!(status, expected, "{uri}: {body}");
    }
}

#[tokio::test]
async fn git_branches_reports_non_git_workspace() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("notes")).unwrap();
    let (state, id, _pontia_home) = workspace_for(root.path(), "notes").await;

    let (status, body) = get_json(state, &format!("/api/v1/workspaces/{id}/git/branches")).await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body["data"]["branches"]["repo_root"].is_null());
    assert_eq!(body["data"]["branches"]["branches"], json!([]));
}
