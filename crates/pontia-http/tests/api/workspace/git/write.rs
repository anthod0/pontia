use super::*;

async fn write(state: &AppState, id: &str, op: &str, request: Value) -> Value {
    let (status, body) = post_json(
        state.clone(),
        &format!("/api/v1/workspaces/{id}/git/{op}"),
        request,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{op}: {body}");
    body["data"].clone()
}

#[tokio::test]
async fn stage_unstage_and_commit_preserve_the_manual_selection() {
    let root = tempfile::tempdir().unwrap();
    let app = root.path().join("app");
    fs::create_dir(&app).unwrap();
    git(&app, &["init", "-qb", "main"]);
    git(&app, &["config", "user.name", "Test"]);
    git(&app, &["config", "user.email", "test@example.com"]);
    fs::write(app.join("one"), "first").unwrap();
    fs::write(app.join("-two"), "second").unwrap();
    let (state, id, _home) = workspace_for(root.path(), "app").await;
    let all = json!({"all":true});
    let staged = write(&state, &id, "stage", json!({"paths":["one","-two"]})).await;
    assert_eq!(staged["git_status"]["staged_count"], 2);
    let cleared = write(&state, &id, "unstage", all.clone()).await;
    assert_eq!(cleared["git_status"]["untracked_count"], 2);
    write(&state, &id, "stage", all).await;
    write(&state, &id, "unstage", json!({"paths":["-two"]})).await;
    fs::write(app.join("one"), "later edit").unwrap();
    let first = write(
        &state,
        &id,
        "commit",
        json!({"message":"first", "stage_all":true}),
    )
    .await;
    assert_eq!(first["commit"]["subject"], "first");
    git(
        &app,
        &[
            "cat-file",
            "-e",
            &format!("{}:one", first["commit"]["sha"].as_str().unwrap()),
        ],
    );
    assert_eq!(first["git_status"]["unstaged_count"], 1);
    assert_eq!(first["git_status"]["untracked_count"], 1);
    let second = write(
        &state,
        &id,
        "commit",
        json!({"message":"second", "stage_all":true}),
    )
    .await;
    assert_eq!(second["git_status"]["clean"], true);
    let (_, stored) = get_json(state, &format!("/api/v1/workspaces/{id}/git-status")).await;
    assert_eq!(stored["data"]["git_status"], second["git_status"]);
}

#[tokio::test]
async fn discard_restores_index_content_and_unlinks_only_selected_files() {
    let root = tempfile::tempdir().unwrap();
    let app = root.path().join("app");
    repo_with_changes(&app);
    let outside = root.path().join("outside");
    fs::write(&outside, "untouched").unwrap();
    std::os::unix::fs::symlink(&outside, app.join("link")).unwrap();
    std::os::unix::fs::symlink(root.path(), app.join("escape")).unwrap();
    let (state, id, _home) = workspace_for(root.path(), "app").await;
    let base = format!("/api/v1/workspaces/{id}/git");
    for path in [
        "../outside",
        "/outside",
        "escape/outside",
        ".git/config",
        "scratch",
        "new name.txt",
    ] {
        let (status, body) = post_json(
            state.clone(),
            &format!("{base}/discard"),
            json!({"paths":["notes.txt",path]}),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{path}: {body}");
        assert!(app.join("notes.txt").exists());
    }
    fs::rename(app.join("src"), app.join("saved-src")).unwrap();
    std::os::unix::fs::symlink(root.path(), app.join("src")).unwrap();
    let (status, _) = post_json(
        state.clone(),
        &format!("{base}/discard"),
        json!({"paths":["src/lib.rs"]}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(!root.path().join("lib.rs").exists());
    assert!(app.join("src").is_symlink());
    fs::remove_file(app.join("src")).unwrap();
    write(
        &state,
        &id,
        "discard",
        json!({"paths":["README.md","src/lib.rs","link","scratch/x"]}),
    )
    .await;
    assert_eq!(
        fs::read_to_string(app.join("README.md")).unwrap(),
        "one\nTWO\nthree\n"
    );
    assert_eq!(
        fs::read_to_string(app.join("src/lib.rs")).unwrap(),
        "fn a() {}\n"
    );
    assert_eq!(fs::read_to_string(outside).unwrap(), "untouched");
    assert!(app.join("link").symlink_metadata().is_err());
    assert!(!app.join("scratch/x").exists());
    assert!(app.join("notes.txt").exists());
}

#[tokio::test]
async fn writes_require_authentication_valid_workspace_and_valid_requests() {
    let root = tempfile::tempdir().unwrap();
    let app = root.path().join("app");
    repo_with_changes(&app);
    let (state, id, _home) = workspace_for(root.path(), "app").await;
    let base = format!("/api/v1/workspaces/{id}/git");
    for op in ["stage", "unstage", "discard", "commit", "pull", "push"] {
        let response = crate::common::http::router(state.clone())
            .oneshot(
                Request::post(format!("{base}/{op}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
    for (op, request) in [
        ("commit", json!({"message":" \n","stage_all":true})),
        ("stage", json!({"paths":[]})),
        ("unstage", json!({"all":true,"paths":["notes.txt"]})),
        ("stage", json!({"paths":["../outside"]})),
    ] {
        let (status, body) = post_json(state.clone(), &format!("{base}/{op}"), request).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    }
    let (status, _) = post_json(
        state,
        "/api/v1/workspaces/missing/git/stage",
        json!({"all":true}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn push_and_pull_fast_forward_only_the_current_branch() {
    let root = tempfile::tempdir().unwrap();
    git(
        root.path(),
        &["init", "--bare", "-qb", "main", "remote.git"],
    );
    git(root.path(), &["clone", "-q", "remote.git", "app"]);
    let app = root.path().join("app");
    fs::write(app.join("base"), "base").unwrap();
    git(&app, &["add", "base"]);
    git(&app, &[COMMIT, &["initial"]].concat());
    git(&app, &["branch", "other"]);
    git(
        &app,
        &[
            "config",
            "remote.origin.push",
            "+refs/heads/other:refs/heads/other",
        ],
    );
    let (state, id, _home) = workspace_for(root.path(), "app").await;
    write(&state, &id, "push", Value::Null).await;
    git(root.path(), &["clone", "-q", "remote.git", "peer"]);
    let peer = root.path().join("peer");
    assert_eq!(fs::read_to_string(peer.join("base")).unwrap(), "base");
    assert!(!root.path().join("remote.git/refs/heads/other").exists());
    fs::write(peer.join("remote"), "incoming").unwrap();
    git(&peer, &["add", "remote"]);
    git(&peer, &[COMMIT, &["remote update"]].concat());
    git(&peer, &["push", "-q"]);
    let updated = write(&state, &id, "pull", Value::Null).await;
    assert_eq!(updated["git_status"]["behind"], 0);
    assert_eq!(fs::read_to_string(app.join("remote")).unwrap(), "incoming");
    for (repo, name) in [(&app, "local-only"), (&peer, "remote-only")] {
        fs::write(repo.join(name), name).unwrap();
        git(repo, &["add", name]);
        git(repo, &[COMMIT, &[name]].concat());
    }
    git(&peer, &["push", "-q"]);
    let read_head = |dir: &Path| fs::read(dir.join("refs/heads/main")).unwrap();
    let remote = root.path().join("remote.git");
    let local_head = read_head(&app.join(".git"));
    let remote_head = read_head(&remote);
    for (op, reason) in [("pull", "fast-forward"), ("push", "rejected")] {
        let (status, body) =
            post_empty(state.clone(), &format!("/api/v1/workspaces/{id}/git/{op}")).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        assert!(
            body["error"]["message"].as_str().unwrap().contains(reason),
            "{body}"
        );
    }
    assert_eq!(read_head(&app.join(".git")), local_head);
    assert_eq!(read_head(&remote), remote_head);
    assert!(!app.join("remote-only").exists());
}

#[tokio::test]
async fn discard_preserves_files_requiring_a_repository_defined_filter() {
    let root = tempfile::tempdir().unwrap();
    let app = root.path().join("app");
    repo_with_changes(&app);
    fs::write(app.join(".gitattributes"), "README.md filter=custom\n").unwrap();
    git(&app, &["config", "filter.custom.smudge", "git stripspace"]);
    let (state, id, _home) = workspace_for(root.path(), "app").await;
    let (status, _) = post_json(
        state.clone(),
        &format!("/api/v1/workspaces/{id}/git/discard"),
        json!({"paths":["README.md","src/lib.rs"]}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        fs::read_to_string(app.join("README.md")).unwrap(),
        "one\nTWO\nthree\nfour\n"
    );
    assert!(!app.join("src/lib.rs").exists());
    write(&state, &id, "discard", json!({"paths":["src/lib.rs"]})).await;
    assert_eq!(
        fs::read_to_string(app.join("src/lib.rs")).unwrap(),
        "fn a() {}\n"
    );
}
