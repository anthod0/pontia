use super::*;

#[tokio::test]
async fn session_creation_requires_explicit_activation_by_path_or_id() {
    let _client = GenericClientTestScope::new().await;
    let root = tempfile::tempdir().unwrap();
    let cwd = root.path().join("project");
    std::fs::create_dir(&cwd).unwrap();
    let (state, _pontia_home) = test_state(vec![WorkspaceRootConfig {
        root_id: "projects".into(),
        label: "Projects".into(),
        path: root.path().display().to_string(),
    }])
    .await;
    for request in [
        json!({"client_type":"generic","workspace":cwd}),
        json!({"client_type":"generic"}),
        json!({"client_type":"generic","workspace":root.path().join("missing")}),
    ] {
        let (status, _) = post_json(state.clone(), "/api/v1/sessions", request).await;
        assert!(status.is_client_error());
    }
    assert!(!root.path().join("missing").exists());
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM workspaces")
        .fetch_one(&state.db())
        .await
        .unwrap();
    assert_eq!(count, 0);
    let (_, body) = post_json(
        state.clone(),
        "/api/v1/workspaces",
        json!({"root_id":"projects","path":"project","name":"Project"}),
    )
    .await;
    let id = body["data"]["workspace"]["workspace_id"].as_str().unwrap();
    for disabled in ["archived", "deleted"] {
        sqlx::query("UPDATE workspaces SET state=? WHERE workspace_id=?")
            .bind(disabled)
            .bind(id)
            .execute(&state.db())
            .await
            .unwrap();
        for request in [
            json!({"client_type":"generic","workspace":cwd}),
            json!({"client_type":"generic","workspace_id":id}),
        ] {
            let (status, _) = post_json(state.clone(), "/api/v1/sessions", request).await;
            assert_eq!(status, StatusCode::CONFLICT);
        }
        let stored: String =
            sqlx::query_scalar("SELECT state FROM workspaces WHERE workspace_id=?")
                .bind(id)
                .fetch_one(&state.db())
                .await
                .unwrap();
        assert_eq!(stored, disabled);
    }
    let sessions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions")
        .fetch_one(&state.db())
        .await
        .unwrap();
    assert_eq!(sessions, 0);
    let (_, registered) = post_json(
        state.clone(),
        "/api/v1/workspaces",
        json!({"root_id":"projects","path":"project","name":"Project"}),
    )
    .await;
    assert_eq!(registered["data"]["workspace"]["workspace_id"], id);
    for request in [
        json!({"client_type":"generic","workspace":cwd}),
        json!({"client_type":"generic","workspace_id":id}),
    ] {
        let (status, body) = post_json(state.clone(), "/api/v1/sessions", request).await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        assert_eq!(body["data"]["session"]["workspace_id"], id);
    }
    let rows: Vec<(String, String)> = sqlx::query_as("SELECT state,name FROM workspaces")
        .fetch_all(&state.db())
        .await
        .unwrap();
    assert_eq!(rows, vec![("active".into(), "Project".into())]);
}
