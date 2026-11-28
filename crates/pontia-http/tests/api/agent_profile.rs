use crate::common::http;
use crate::common::test_app::TestApp;
use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use pontia_application::AppState;
use serde_json::{Value, json};
use tower::ServiceExt;

const TOKEN: &str = "test-token";

async fn test_state() -> (AppState, tempfile::TempDir) {
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

fn profile(id: &str) -> Value {
    json!({"profile_id": id, "name": "Reviewer", "description":"Reviews changes", "system_prompt":"Review carefully"})
}

#[tokio::test]
async fn profile_crud_and_idempotent_mutations() {
    let (state, _pontia_home) = test_state().await;
    assert_eq!(
        get_json(state.clone(), "/api/v1/agent-profiles").await.1["data"]["agent_profiles"],
        json!([])
    );
    let (status, created) = post_json(
        state.clone(),
        "/api/v1/agent-profiles",
        profile("reviewer"),
        Some("create"),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    assert_eq!(created["data"]["agent_profile"]["use_yolo"], false);
    let (status, replay) = post_json(
        state.clone(),
        "/api/v1/agent-profiles",
        profile("reviewer"),
        Some("create"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(created["data"], replay["data"]);
    assert_eq!(
        post_json(
            state.clone(),
            "/api/v1/agent-profiles",
            profile("reviewer"),
            None
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let uri = "/api/v1/agent-profiles/reviewer";
    let mut edited = profile("reviewer");
    edited["name"] = json!("Edited");
    edited["system_prompt"] = json!("New instructions");
    let (status, updated) = put_json(state.clone(), uri, edited.clone(), Some("update")).await;
    assert_eq!(status, StatusCode::OK, "{updated}");
    assert_eq!(
        put_json(state.clone(), uri, edited, Some("update")).await.1["data"],
        updated["data"]
    );
    let fetched = get_json(state.clone(), uri).await.1;
    assert_eq!(
        fetched["data"]["agent_profile"]["system_prompt"],
        "New instructions"
    );
    assert_eq!(
        fetched["data"]["agent_profile"]["created_at"],
        created["data"]["agent_profile"]["created_at"]
    );
    assert_eq!(
        get_json(state.clone(), "/api/v1/agent-profiles").await.1["data"]["agent_profiles"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        delete_json(state.clone(), uri, Some("delete")).await.0,
        StatusCode::OK
    );
    assert_eq!(
        delete_json(state.clone(), uri, Some("delete")).await.0,
        StatusCode::OK
    );
    assert_eq!(
        delete_json(state.clone(), uri, None).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(get_json(state.clone(), uri).await.0, StatusCode::NOT_FOUND);
    assert_eq!(
        put_json(state.clone(), uri, profile("reviewer"), None)
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        post_json(state, "/api/v1/agent-profiles", profile("reviewer"), None)
            .await
            .0,
        StatusCode::CREATED
    );
}

#[tokio::test]
async fn profile_launch_defaults_and_validation() {
    let (state, _pontia_home) = test_state().await;
    let mut input = profile("configured");
    input["client_type"] = json!("pi");
    input["client_key"] = json!("default");
    assert_eq!(
        post_json(state.clone(), "/api/v1/agent-profiles", input.clone(), None)
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    input["model"] = json!("provider/model");
    input["system_prompt"] = json!("{{input}}");
    let (status, created) = post_json(state.clone(), "/api/v1/agent-profiles", input, None).await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    assert_eq!(created["data"]["agent_profile"]["model"], "provider/model");
    assert_eq!(
        created["data"]["agent_profile"]["system_prompt"],
        "{{input}}"
    );
    for (field, value) in [("profile_id", " "), ("name", " "), ("system_prompt", " ")] {
        let mut input = profile("invalid");
        input[field] = json!(value);
        assert_eq!(
            post_json(state.clone(), "/api/v1/agent-profiles", input, None)
                .await
                .0,
            StatusCode::BAD_REQUEST
        );
    }
    assert_eq!(
        put_json(
            state.clone(),
            "/api/v1/agent-profiles/configured",
            profile("different"),
            None
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    let mut obsolete = profile("obsolete");
    obsolete["version"] = json!("1");
    let response = http::router(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/agent-profiles")
                .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(obsolete.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    for uri in [
        "/api/v1/agent-profiles/configured/versions",
        "/api/v1/agent-profiles/configured/versions/1",
    ] {
        let response = http::router(state.clone())
            .oneshot(
                Request::builder()
                    .uri(uri)
                    .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
