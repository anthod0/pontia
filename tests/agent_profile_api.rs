use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use llmparty::{
    application::AppState,
    storage::sqlite::{connect_sqlite, run_migrations},
    transport::http,
};
use serde_json::{Value, json};
use tower::ServiceExt;

const TOKEN: &str = "test-token";

async fn test_state() -> AppState {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("agent_profiles.db");
    let _kept_dir = dir.keep();
    let database_url = format!("sqlite://{}", db_path.display());
    let db = connect_sqlite(&database_url).await.expect("connect");
    run_migrations(&db).await.expect("migrate");
    AppState {
        db,
        external_api_token: Some(TOKEN.to_string()),
        planner: Default::default(),
        graph: Default::default(),
        workspace_browser: Default::default(),
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

async fn post_json(
    state: AppState,
    uri: &str,
    body: Value,
    idempotency_key: Option<&str>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method("POST")
        .uri(uri)
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(key) = idempotency_key {
        builder = builder.header("Idempotency-Key", key);
    }

    let response = http::router(state)
        .oneshot(builder.body(Body::from(body.to_string())).expect("request"))
        .await
        .expect("response");

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

fn custom_profile_body(profile_id: &str, version: &str) -> Value {
    json!({
        "profile_id": profile_id,
        "version": version,
        "name": "API Reviewer",
        "description": "Reviews backend API changes.",
        "supported_client_types": ["pi", "claude_code"],
        "system_prompt_template": "You review API changes.",
        "turn_prompt_template": "Review {{work_item_id}}: {{title}}",
        "default_session_role": "API reviewer",
        "default_session_description": "Reviews backend API WorkItems.",
        "handle_prefix": "api-review",
        "session_reuse_policy": "fresh_per_work_item",
        "expected_output_schema": "review_result_v1",
        "artifact_contract": { "produces": ["review"] },
        "default_execution_policy": { "allow_file_writes": false },
        "default_review_policy": { "required": false },
        "metadata": { "team": "platform" }
    })
}

#[tokio::test]
async fn list_agent_profiles_includes_builtin_latest_profiles() {
    let state = test_state().await;

    let (status, body) = get_json(state, "/external/v1/agent-profiles").await;

    assert_eq!(status, StatusCode::OK, "{body}");
    let profiles = body["data"]["agent_profiles"].as_array().unwrap();
    for expected in [
        "default",
        "planner",
        "replanner",
        "implementer",
        "reviewer",
        "tester",
        "debugger",
    ] {
        assert!(
            profiles
                .iter()
                .any(|profile| profile["profile_id"] == expected),
            "missing {expected}: {profiles:?}"
        );
    }
    let default = profiles
        .iter()
        .find(|profile| profile["profile_id"] == "default")
        .unwrap();
    assert_eq!(default["version"], "1");
    assert_eq!(
        default["session_reuse_policy"],
        "reuse_by_workspace_and_profile"
    );
}

#[tokio::test]
async fn get_agent_profile_returns_latest_version() {
    let state = test_state().await;

    let (status, body) = get_json(state, "/external/v1/agent-profiles/replanner").await;

    assert_eq!(status, StatusCode::OK, "{body}");
    let profile = &body["data"]["agent_profile"];
    assert_eq!(profile["profile_id"], "replanner");
    assert_eq!(profile["expected_output_schema"], "dag_patch_v1");
    assert_eq!(profile["session_reuse_policy"], "fresh_per_run");
}

#[tokio::test]
async fn create_agent_profile_persists_and_is_idempotent() {
    let state = test_state().await;
    let body = custom_profile_body("api-reviewer", "1");

    let (status, created) = post_json(
        state.clone(),
        "/external/v1/agent-profiles",
        body.clone(),
        Some("create-api-reviewer"),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let profile = &created["data"]["agent_profile"];
    assert_eq!(profile["profile_id"], "api-reviewer");
    assert_eq!(
        profile["supported_client_types"],
        json!(["pi", "claude_code"])
    );
    assert_eq!(profile["metadata"], json!({ "team": "platform" }));

    let (status, replay) = post_json(
        state.clone(),
        "/external/v1/agent-profiles",
        body,
        Some("create-api-reviewer"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{replay}");
    assert_eq!(replay["data"], created["data"]);

    let (status, fetched) = get_json(state, "/external/v1/agent-profiles/api-reviewer").await;
    assert_eq!(status, StatusCode::OK, "{fetched}");
    assert_eq!(fetched["data"]["agent_profile"]["version"], "1");
}

#[tokio::test]
async fn add_agent_profile_version_updates_latest_without_modifying_previous_versions() {
    let state = test_state().await;
    let body_v1 = custom_profile_body("release-reviewer", "1");
    let (status, _) = post_json(state.clone(), "/external/v1/agent-profiles", body_v1, None).await;
    assert_eq!(status, StatusCode::CREATED);

    let mut body_v2 = custom_profile_body("release-reviewer", "2");
    body_v2["name"] = json!("Release Reviewer v2");
    body_v2["expected_output_schema"] = json!("release_review_v2");
    let (status, created) = post_json(
        state.clone(),
        "/external/v1/agent-profiles/release-reviewer/versions",
        body_v2,
        Some("release-reviewer-v2"),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    assert_eq!(created["data"]["agent_profile"]["version"], "2");

    let (status, fetched) = get_json(state, "/external/v1/agent-profiles/release-reviewer").await;
    assert_eq!(status, StatusCode::OK, "{fetched}");
    assert_eq!(
        fetched["data"]["agent_profile"]["name"],
        "Release Reviewer v2"
    );
    assert_eq!(
        fetched["data"]["agent_profile"]["expected_output_schema"],
        "release_review_v2"
    );
}

#[tokio::test]
async fn duplicate_agent_profile_version_returns_conflict() {
    let state = test_state().await;
    let body = custom_profile_body("duplicate-reviewer", "1");
    let (status, _) = post_json(
        state.clone(),
        "/external/v1/agent-profiles",
        body.clone(),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, duplicate) = post_json(state, "/external/v1/agent-profiles", body, None).await;

    assert_eq!(status, StatusCode::CONFLICT, "{duplicate}");
    assert_eq!(duplicate["error"]["code"], "state_conflict");
}

#[tokio::test]
async fn missing_agent_profile_returns_not_found() {
    let state = test_state().await;

    let (status, body) = get_json(state, "/external/v1/agent-profiles/missing-profile").await;

    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["error"]["code"], "not_found");
}
