use crate::common::test_app::TestApp;
use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use pontia_application::AppState;
use pontia_http as http;
use serde_json::{Value, json};
use tower::ServiceExt;

const TOKEN: &str = "test-token";

async fn test_state() -> AppState {
    TestApp::builder()
        .database_name("agent_profiles.db")
        .external_api_token(Some(TOKEN.to_string()))
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
        "supported_client_types": ["pi"],
        "agent_kind": "executor",
        "system_prompt_template": "You review API changes.",
        "turn_prompt_template": "Review {{input}}",
        "default_session_role": "API reviewer",
        "default_session_description": "Reviews backend API changes.",
        "handle_prefix": "api-review",
        "expected_output_schema": "review_result_v1",
        "artifact_contract": { "produces": ["review"] },
        "default_execution_policy": { "allow_file_writes": false },
        "default_review_policy": { "required": false },
        "metadata": { "team": "platform" }
    })
}

#[tokio::test]
async fn profile_client_support_remains_optional_without_launch_defaults() {
    let state = test_state().await;
    let profile = json!({"profile_id":"instructions-only", "version":"1", "name":"Instructions", "agent_kind":"executor"});
    let (status, created) = post_json(state, "/api/v1/agent-profiles", profile, None).await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    assert_eq!(
        created["data"]["agent_profile"]["supported_client_types"],
        json!([])
    );
    assert!(created["data"]["agent_profile"]["default_client_type"].is_null());
}

#[tokio::test]
async fn profile_launch_defaults_round_trip_and_reject_partial_configuration() {
    let state = test_state().await;
    let mut profile = custom_profile_body("helper-defaults", "1");
    profile["default_client_type"] = json!("pi");
    profile["default_client_key"] = json!("default");
    let (status, _) = post_json(
        state.clone(),
        "/api/v1/agent-profiles",
        profile.clone(),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    profile["default_model"] = json!("provider/creation-model");
    let (status, created) = post_json(
        state.clone(),
        "/api/v1/agent-profiles",
        profile.clone(),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    assert_eq!(
        created["data"]["agent_profile"]["default_model"],
        "provider/creation-model"
    );
    profile["default_model"] = json!("provider/edited-model");
    let (status, updated) = put_json(
        state.clone(),
        "/api/v1/agent-profiles/helper-defaults/versions/1",
        profile,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{updated}");
    let (status, fetched) = get_json(state, "/api/v1/agent-profiles/helper-defaults").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        fetched["data"]["agent_profile"]["default_client_type"],
        "pi"
    );
    assert_eq!(
        fetched["data"]["agent_profile"]["default_client_key"],
        "default"
    );
    assert_eq!(
        fetched["data"]["agent_profile"]["default_model"],
        "provider/edited-model"
    );
}

#[tokio::test]
async fn list_agent_profiles_starts_empty_without_builtin_profiles() {
    let state = test_state().await;

    let (status, body) = get_json(state, "/api/v1/agent-profiles").await;

    assert_eq!(status, StatusCode::OK, "{body}");
    let profiles = body["data"]["agent_profiles"].as_array().unwrap();
    assert!(
        profiles.is_empty(),
        "unexpected default profiles: {profiles:?}"
    );
}

#[tokio::test]
async fn get_agent_profile_returns_latest_version() {
    let state = test_state().await;
    let (status, created) = post_json(
        state.clone(),
        "/api/v1/agent-profiles",
        custom_profile_body("latest-check", "1"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let (status, created_v2) = post_json(
        state.clone(),
        "/api/v1/agent-profiles/latest-check/versions",
        custom_profile_body("latest-check", "2"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created_v2}");

    let (status, body) = get_json(state, "/api/v1/agent-profiles/latest-check").await;

    assert_eq!(status, StatusCode::OK, "{body}");
    let profile = &body["data"]["agent_profile"];
    assert_eq!(profile["profile_id"], "latest-check");
    assert_eq!(profile["version"], "2");
    assert_eq!(profile["agent_kind"], "executor");
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
        "/api/v1/agent-profiles",
        body.clone(),
        Some("create-api-reviewer"),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let profile = &created["data"]["agent_profile"];
    assert_eq!(profile["profile_id"], "api-reviewer");
    assert_eq!(profile["agent_kind"], "executor");
    assert_eq!(profile["supported_client_types"], json!(["pi"]));
    assert_eq!(profile["metadata"], json!({ "team": "platform" }));

    let (status, replay) = post_json(
        state.clone(),
        "/api/v1/agent-profiles",
        body,
        Some("create-api-reviewer"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{replay}");
    assert_eq!(replay["data"], created["data"]);

    let (status, fetched) = get_json(state, "/api/v1/agent-profiles/api-reviewer").await;
    assert_eq!(status, StatusCode::OK, "{fetched}");
    assert_eq!(fetched["data"]["agent_profile"]["version"], "1");
}

#[tokio::test]
async fn create_agent_profile_rejects_unsupported_agent_kind() {
    let state = test_state().await;
    let mut body = custom_profile_body("invalid-kind", "1");
    body["agent_kind"] = json!("unsupported");

    let (status, response) = post_json(state, "/api/v1/agent-profiles", body, None).await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "{response}");
    assert_eq!(response["error"]["code"], "invalid_request");
}

#[tokio::test]
async fn add_agent_profile_version_updates_latest_without_modifying_previous_versions() {
    let state = test_state().await;
    let body_v1 = custom_profile_body("release-reviewer", "1");
    let (status, _) = post_json(state.clone(), "/api/v1/agent-profiles", body_v1, None).await;
    assert_eq!(status, StatusCode::CREATED);

    let mut body_v2 = custom_profile_body("release-reviewer", "2");
    body_v2["name"] = json!("Release Reviewer v2");
    body_v2["expected_output_schema"] = json!("release_review_v2");
    let (status, created) = post_json(
        state.clone(),
        "/api/v1/agent-profiles/release-reviewer/versions",
        body_v2,
        Some("release-reviewer-v2"),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    assert_eq!(created["data"]["agent_profile"]["version"], "2");

    let (status, fetched) = get_json(state, "/api/v1/agent-profiles/release-reviewer").await;
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
    let (status, _) = post_json(state.clone(), "/api/v1/agent-profiles", body_v1, None).await;
    assert_eq!(status, StatusCode::CREATED);
    let mut body_v2 = custom_profile_body("versioned-reviewer", "2");
    body_v2["name"] = json!("Versioned Reviewer v2");
    let (status, _) = post_json(
        state.clone(),
        "/api/v1/agent-profiles/versioned-reviewer/versions",
        body_v2,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, listed) = get_json(
        state.clone(),
        "/api/v1/agent-profiles/versioned-reviewer/versions",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    let versions = listed["data"]["agent_profile_versions"].as_array().unwrap();
    assert_eq!(versions.len(), 2);
    assert_eq!(versions[0]["version"], "1");
    assert_eq!(versions[1]["version"], "2");

    let (status, fetched) = get_json(
        state,
        "/api/v1/agent-profiles/versioned-reviewer/versions/1",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{fetched}");
    assert_eq!(fetched["data"]["agent_profile"]["version"], "1");
}

#[tokio::test]
async fn updates_agent_profile_version_with_put() {
    let state = test_state().await;
    let body = custom_profile_body("editable-reviewer", "1");
    let (status, _) = post_json(state.clone(), "/api/v1/agent-profiles", body, None).await;
    assert_eq!(status, StatusCode::CREATED);

    let mut updated = custom_profile_body("editable-reviewer", "1");
    updated["name"] = json!("Edited Reviewer");
    updated["metadata"] = json!({"team":"platform", "edited": true});
    let (status, body) = put_json(
        state.clone(),
        "/api/v1/agent-profiles/editable-reviewer/versions/1",
        updated,
        Some("edit-reviewer-v1"),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["agent_profile"]["name"], "Edited Reviewer");
    assert_eq!(body["data"]["agent_profile"]["metadata"]["edited"], true);

    let (status, fetched) = get_json(state, "/api/v1/agent-profiles/editable-reviewer").await;
    assert_eq!(status, StatusCode::OK, "{fetched}");
    assert_eq!(fetched["data"]["agent_profile"]["name"], "Edited Reviewer");
}

#[tokio::test]
async fn deleting_latest_version_archives_it_and_falls_back_to_previous_version() {
    let state = test_state().await;
    let body_v1 = custom_profile_body("archive-version-reviewer", "1");
    let (status, _) = post_json(state.clone(), "/api/v1/agent-profiles", body_v1, None).await;
    assert_eq!(status, StatusCode::CREATED);
    let mut body_v2 = custom_profile_body("archive-version-reviewer", "2");
    body_v2["name"] = json!("Archive Version Reviewer v2");
    let (status, _) = post_json(
        state.clone(),
        "/api/v1/agent-profiles/archive-version-reviewer/versions",
        body_v2,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, deleted) = delete_json(
        state.clone(),
        "/api/v1/agent-profiles/archive-version-reviewer/versions/2",
        Some("archive-v2"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{deleted}");
    assert_eq!(deleted["data"]["agent_profile"]["active"], false);
    assert!(deleted["data"]["agent_profile"]["archived_at"].is_string());

    let (status, latest) = get_json(
        state.clone(),
        "/api/v1/agent-profiles/archive-version-reviewer",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{latest}");
    assert_eq!(latest["data"]["agent_profile"]["version"], "1");

    let (status, versions) = get_json(
        state,
        "/api/v1/agent-profiles/archive-version-reviewer/versions?include_archived=true",
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
    let (status, _) = post_json(state.clone(), "/api/v1/agent-profiles", body, None).await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, deleted) = delete_json(
        state.clone(),
        "/api/v1/agent-profiles/archive-all-reviewer",
        Some("archive-profile"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{deleted}");
    assert_eq!(deleted["data"]["archived_versions"], 1);

    let (status, missing) =
        get_json(state.clone(), "/api/v1/agent-profiles/archive-all-reviewer").await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{missing}");

    let (status, listed) = get_json(
        state.clone(),
        "/api/v1/agent-profiles?include_archived=true",
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
        "/api/v1/agent-profiles/archive-all-reviewer/versions?include_archived=true",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{versions}");
    assert_eq!(
        versions["data"]["agent_profile_versions"][0]["active"],
        false
    );
}

#[tokio::test]
async fn duplicate_agent_profile_version_returns_conflict() {
    let state = test_state().await;
    let body = custom_profile_body("duplicate-reviewer", "1");
    let (status, _) = post_json(state.clone(), "/api/v1/agent-profiles", body.clone(), None).await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, duplicate) = post_json(state, "/api/v1/agent-profiles", body, None).await;

    assert_eq!(status, StatusCode::CONFLICT, "{duplicate}");
    assert_eq!(duplicate["error"]["code"], "state_conflict");
}

#[tokio::test]
async fn missing_agent_profile_returns_not_found() {
    let state = test_state().await;

    let (status, body) = get_json(state, "/api/v1/agent-profiles/missing-profile").await;

    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(body["error"]["code"], "not_found");
}

#[tokio::test]
async fn codex_instruction_snapshot_rejects_invalid_contracts() {
    let app = TestApp::new().await;
    let (status, created) = post_json(app.state.clone(), "/api/v1/sessions",
        json!({"client_type":"codex", "workspace":app.workspace().path(), "system_instructions":"review-marker"}), None).await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let session = created["data"]["session"]["session_id"].as_str().unwrap();
    let instructions = pontia_client_codex::instructions::CodexInstructions::new(app.db.clone());
    assert_eq!(
        instructions
            .codex_binding(session)
            .await
            .unwrap()
            .unwrap()
            .system_prompt
            .as_deref(),
        Some("review-marker")
    );
    sqlx::query("UPDATE events SET payload=json_set(payload,'$.session_instructions.contract_version',2) WHERE session_id=? AND event_type='session.created'")
        .bind(session).execute(&app.db).await.unwrap();
    assert!(instructions.codex_binding(session).await.is_err());
}

#[tokio::test]
async fn standalone_session_instructions_are_opt_in_and_fixed_at_creation() {
    let app = TestApp::new().await;
    let service = pontia_agent_control::SessionInstructionsService::new(app.db.clone());
    let standalone = pontia_agent_control::standalone_session_instructions();

    // Standalone session instructions default to off.
    let (status, omitted) = post_json(
        app.state.clone(),
        "/api/v1/sessions",
        json!({"client_type":"codex", "workspace":app.workspace().path()}),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{omitted}");
    let omitted_session = omitted["data"]["session"]["session_id"].as_str().unwrap();
    assert!(
        service
            .session_instructions(omitted_session)
            .await
            .unwrap()
            .is_none()
    );

    let (status, caller_only) = post_json(
        app.state.clone(),
        "/api/v1/sessions",
        json!({
            "client_type": "codex",
            "workspace": app.workspace().path(),
            "system_instructions": "caller only"
        }),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{caller_only}");
    let caller_only_session = caller_only["data"]["session"]["session_id"]
        .as_str()
        .unwrap();
    assert_eq!(
        service
            .session_instructions(caller_only_session)
            .await
            .unwrap()
            .unwrap()["system_prompt"],
        "caller only"
    );

    let request = json!({
        "client_type": "codex",
        "workspace": app.workspace().path(),
        "include_standalone_session_instructions": true
    });
    let (status, created) = post_json(app.state.clone(), "/api/v1/sessions", request, None).await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let session = created["data"]["session"]["session_id"].as_str().unwrap();
    assert!(created["data"]["session"]["execution_profile_id"].is_null());
    assert!(created["data"]["session"]["execution_profile_version"].is_null());
    let snapshot = service
        .session_instructions(session)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(snapshot["system_prompt"], standalone);

    // Removing the mutable source proves recovery cannot read it.
    sqlx::query("DROP TABLE execution_profiles")
        .execute(&app.db)
        .await
        .unwrap();
    let reopened = pontia_client_codex::instructions::CodexInstructions::new(app.db.clone());
    assert_eq!(
        reopened
            .codex_binding(session)
            .await
            .unwrap()
            .unwrap()
            .system_prompt
            .as_deref(),
        Some(standalone)
    );
    sqlx::query("UPDATE events SET payload=json_remove(payload,'$.session_instructions') WHERE session_id=? AND event_type='session.created'")
        .bind(session).execute(&app.db).await.unwrap();
    assert!(reopened.codex_binding(session).await.unwrap().is_none());
}

#[tokio::test]
async fn system_instructions_without_explicit_profile_are_isolated_per_session() {
    let app = TestApp::new().await;
    let standalone = pontia_agent_control::standalone_session_instructions();
    let service = pontia_agent_control::SessionInstructionsService::new(app.db.clone());
    for (extra, expected) in [
        (
            Some("first caller"),
            format!("{standalone}\n\nfirst caller"),
        ),
        (
            Some("second caller"),
            format!("{standalone}\n\nsecond caller"),
        ),
        (None, standalone.to_owned()),
    ] {
        let (status, created) = post_json(
            app.state.clone(),
            "/api/v1/sessions",
            json!({
                "client_type": "codex",
                "workspace": app.workspace().path(),
                "system_instructions": extra,
                "include_standalone_session_instructions": true
            }),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{created}");
        let session = created["data"]["session"]["session_id"].as_str().unwrap();
        assert_eq!(
            service
                .session_instructions(session)
                .await
                .unwrap()
                .unwrap()["system_prompt"],
            expected
        );
    }
}

#[tokio::test]
async fn invalid_system_instructions_are_rejected_before_session_creation() {
    let app = TestApp::new().await;
    for (client, extra) in [
        ("codex", json!("")),
        ("codex", json!("Hello {{name}}")),
        ("codex", json!("name}}")),
        ("codex", json!(" \n\t")),
        ("generic", json!("caller instructions")),
    ] {
        let (status, response) = post_json(
            app.state.clone(),
            "/api/v1/sessions",
            json!({
                "client_type": client, "workspace": app.workspace().path(),
                "system_instructions": extra
            }),
            None,
        )
        .await;
        assert!(status.is_client_error(), "{status}: {response}");
    }
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(count, 0);
}
