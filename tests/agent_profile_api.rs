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
        graph: Default::default(),
        workspace_browser: Default::default(),
        dashboard: llmparty::transport::http::dashboard::ResolvedDashboard::local_default(),
        shutdown: Default::default(),
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
    mutating_json(state, "POST", uri, Some(body), idempotency_key).await
}

async fn put_json(
    state: AppState,
    uri: &str,
    body: Value,
    idempotency_key: Option<&str>,
) -> (StatusCode, Value) {
    mutating_json(state, "PUT", uri, Some(body), idempotency_key).await
}

async fn delete_json(
    state: AppState,
    uri: &str,
    idempotency_key: Option<&str>,
) -> (StatusCode, Value) {
    mutating_json(state, "DELETE", uri, None, idempotency_key).await
}

async fn mutating_json(
    state: AppState,
    method: &str,
    uri: &str,
    body: Option<Value>,
    idempotency_key: Option<&str>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"));
    if body.is_some() {
        builder = builder.header(header::CONTENT_TYPE, "application/json");
    }
    if let Some(key) = idempotency_key {
        builder = builder.header("Idempotency-Key", key);
    }

    let response = http::router(state)
        .oneshot(
            builder
                .body(body.map_or_else(Body::empty, |body| Body::from(body.to_string())))
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

fn custom_profile_body(profile_id: &str, version: &str) -> Value {
    json!({
        "profile_id": profile_id,
        "version": version,
        "name": "API Reviewer",
        "description": "Reviews backend API changes.",
        "supported_client_types": ["pi", "claude_code"],
        "agent_kind": "executor",
        "system_prompt_template": "You review API changes.",
        "turn_prompt_template": "Review {{work_item_id}}: {{title}}",
        "default_session_role": "API reviewer",
        "default_session_description": "Reviews backend API WorkItems.",
        "handle_prefix": "api-review",
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
    for expected in ["default", "planner", "replanner", "implementer", "reviewer"] {
        assert!(
            profiles
                .iter()
                .any(|profile| profile["profile_id"] == expected),
            "missing {expected}: {profiles:?}"
        );
    }
    for removed in ["tester", "debugger"] {
        assert!(
            profiles.iter().all(|profile| profile["profile_id"] != removed),
            "removed builtin profile {removed} should not be present: {profiles:?}"
        );
    }
    let default = profiles
        .iter()
        .find(|profile| profile["profile_id"] == "default")
        .unwrap();
    let planner = profiles
        .iter()
        .find(|profile| profile["profile_id"] == "planner")
        .unwrap();
    let replanner = profiles
        .iter()
        .find(|profile| profile["profile_id"] == "replanner")
        .unwrap();
    let implementer = profiles
        .iter()
        .find(|profile| profile["profile_id"] == "implementer")
        .unwrap();
    assert_eq!(default["version"], "1");
    assert_eq!(default["agent_kind"], "executor");
    assert_eq!(planner["agent_kind"], "planner");
    assert_eq!(replanner["agent_kind"], "planner");
    assert_eq!(implementer["agent_kind"], "executor");
    assert!(
        default.get("session_reuse_policy").is_none(),
        "session reuse policy must not be exposed: {default:?}"
    );
}

#[tokio::test]
async fn get_agent_profile_returns_latest_version() {
    let state = test_state().await;

    let (status, body) = get_json(state, "/external/v1/agent-profiles/replanner").await;

    assert_eq!(status, StatusCode::OK, "{body}");
    let profile = &body["data"]["agent_profile"];
    assert_eq!(profile["profile_id"], "replanner");
    assert_eq!(profile["agent_kind"], "planner");
    assert_eq!(profile["expected_output_schema"], "dag_patch_v1");
    assert!(
        profile.get("session_reuse_policy").is_none(),
        "session reuse policy must not be exposed: {profile:?}"
    );
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
    assert_eq!(profile["agent_kind"], "executor");
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
async fn create_agent_profile_rejects_unknown_agent_kind() {
    let state = test_state().await;
    let mut body = custom_profile_body("invalid-kind", "1");
    body["agent_kind"] = json!("plannerish");

    let (status, response) = post_json(state, "/external/v1/agent-profiles", body, None).await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "{response}");
    assert_eq!(response["error"]["code"], "invalid_request");
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
async fn lists_and_gets_exact_agent_profile_versions() {
    let state = test_state().await;
    let body_v1 = custom_profile_body("versioned-reviewer", "1");
    let (status, _) = post_json(state.clone(), "/external/v1/agent-profiles", body_v1, None).await;
    assert_eq!(status, StatusCode::CREATED);
    let mut body_v2 = custom_profile_body("versioned-reviewer", "2");
    body_v2["name"] = json!("Versioned Reviewer v2");
    let (status, _) = post_json(
        state.clone(),
        "/external/v1/agent-profiles/versioned-reviewer/versions",
        body_v2,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, listed) = get_json(
        state.clone(),
        "/external/v1/agent-profiles/versioned-reviewer/versions",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    let versions = listed["data"]["agent_profile_versions"].as_array().unwrap();
    assert_eq!(versions.len(), 2);
    assert_eq!(versions[0]["version"], "1");
    assert_eq!(versions[1]["version"], "2");

    let (status, fetched) = get_json(
        state,
        "/external/v1/agent-profiles/versioned-reviewer/versions/1",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{fetched}");
    assert_eq!(fetched["data"]["agent_profile"]["version"], "1");
}

#[tokio::test]
async fn updates_agent_profile_version_with_put() {
    let state = test_state().await;
    let body = custom_profile_body("editable-reviewer", "1");
    let (status, _) = post_json(state.clone(), "/external/v1/agent-profiles", body, None).await;
    assert_eq!(status, StatusCode::CREATED);

    let mut updated = custom_profile_body("editable-reviewer", "1");
    updated["name"] = json!("Edited Reviewer");
    updated["metadata"] = json!({"team":"platform", "edited": true});
    let (status, body) = put_json(
        state.clone(),
        "/external/v1/agent-profiles/editable-reviewer/versions/1",
        updated,
        Some("edit-reviewer-v1"),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["agent_profile"]["name"], "Edited Reviewer");
    assert_eq!(body["data"]["agent_profile"]["metadata"]["edited"], true);

    let (status, fetched) = get_json(state, "/external/v1/agent-profiles/editable-reviewer").await;
    assert_eq!(status, StatusCode::OK, "{fetched}");
    assert_eq!(fetched["data"]["agent_profile"]["name"], "Edited Reviewer");
}

#[tokio::test]
async fn deleting_latest_version_archives_it_and_falls_back_to_previous_version() {
    let state = test_state().await;
    let body_v1 = custom_profile_body("archive-version-reviewer", "1");
    let (status, _) = post_json(state.clone(), "/external/v1/agent-profiles", body_v1, None).await;
    assert_eq!(status, StatusCode::CREATED);
    let mut body_v2 = custom_profile_body("archive-version-reviewer", "2");
    body_v2["name"] = json!("Archive Version Reviewer v2");
    let (status, _) = post_json(
        state.clone(),
        "/external/v1/agent-profiles/archive-version-reviewer/versions",
        body_v2,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, deleted) = delete_json(
        state.clone(),
        "/external/v1/agent-profiles/archive-version-reviewer/versions/2",
        Some("archive-v2"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{deleted}");
    assert_eq!(deleted["data"]["agent_profile"]["active"], false);
    assert!(deleted["data"]["agent_profile"]["archived_at"].is_string());

    let (status, latest) = get_json(
        state.clone(),
        "/external/v1/agent-profiles/archive-version-reviewer",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{latest}");
    assert_eq!(latest["data"]["agent_profile"]["version"], "1");

    let (status, versions) = get_json(
        state,
        "/external/v1/agent-profiles/archive-version-reviewer/versions?include_archived=true",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{versions}");
    assert_eq!(
        versions["data"]["agent_profile_versions"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[tokio::test]
async fn deleting_profile_archives_custom_versions_and_hides_latest() {
    let state = test_state().await;
    let body = custom_profile_body("archive-all-reviewer", "1");
    let (status, _) = post_json(state.clone(), "/external/v1/agent-profiles", body, None).await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, deleted) = delete_json(
        state.clone(),
        "/external/v1/agent-profiles/archive-all-reviewer",
        Some("archive-profile"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{deleted}");
    assert_eq!(deleted["data"]["archived_versions"], 1);

    let (status, missing) = get_json(
        state.clone(),
        "/external/v1/agent-profiles/archive-all-reviewer",
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{missing}");

    let (status, listed) = get_json(
        state.clone(),
        "/external/v1/agent-profiles?include_archived=true",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    assert!(
        listed["data"]["agent_profiles"]
            .as_array()
            .unwrap()
            .iter()
            .any(|profile| {
                profile["profile_id"] == "archive-all-reviewer" && profile["active"] == false
            })
    );

    let (status, versions) = get_json(
        state,
        "/external/v1/agent-profiles/archive-all-reviewer/versions?include_archived=true",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{versions}");
    assert_eq!(
        versions["data"]["agent_profile_versions"][0]["active"],
        false
    );
}

#[tokio::test]
async fn builtin_agent_profiles_cannot_be_updated_or_deleted() {
    let state = test_state().await;
    let mut body = custom_profile_body("planner", "1");
    body["name"] = json!("Custom Planner");

    let (status, updated) = put_json(
        state.clone(),
        "/external/v1/agent-profiles/planner/versions/1",
        body,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{updated}");

    let (status, deleted) = delete_json(state, "/external/v1/agent-profiles/planner", None).await;
    assert_eq!(status, StatusCode::CONFLICT, "{deleted}");
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
