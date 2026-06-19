use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use pontia::application::AppState;
use pontia::transport::http;
use pontia_storage_sqlite::{connect_sqlite, run_migrations};
use serde_json::{Value, json};
use tower::ServiceExt;

#[path = "../support/generic_client.rs"]
mod generic_client;

use generic_client::GenericClientTestScope;

const TOKEN: &str = "test-token";

async fn test_state(name: &str) -> AppState {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join(format!("{name}.db"));
    let _kept_dir = dir.keep();
    let database_url = format!("sqlite://{}", db_path.display());
    let db = connect_sqlite(&database_url).await.expect("connect");
    run_migrations(&db).await.expect("migrate");
    AppState::builder(db)
        .external_api_token(Some(TOKEN.to_string()))
        .build()
}

async fn request_json(
    state: AppState,
    method: &str,
    uri: &str,
    token: Option<&str>,
    idempotency_key: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(token) = token {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    if let Some(key) = idempotency_key {
        builder = builder.header("Idempotency-Key", key);
    }

    let body = if let Some(body) = body {
        builder = builder.header(header::CONTENT_TYPE, "application/json");
        Body::from(body.to_string())
    } else {
        Body::empty()
    };

    let response = http::router(state)
        .oneshot(builder.body(body).expect("request"))
        .await
        .expect("response");
    response_json(response).await
}

async fn response_json(response: axum::response::Response) -> (StatusCode, Value) {
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

async fn create_session_with_key(state: AppState, key: &str) -> (StatusCode, Value) {
    request_json(
        state,
        "POST",
        "/external/v1/sessions",
        Some(TOKEN),
        Some(key),
        Some(json!({"client_type":"generic","workspace":"/tmp/pontia-mvp"})),
    )
    .await
}

async fn submit_turn_with_key(
    state: AppState,
    session_id: &str,
    key: &str,
    input: &str,
) -> (StatusCode, Value) {
    let (status, body) = request_json(
        state.clone(),
        "POST",
        &format!("/external/v1/sessions/{session_id}/inbox/messages"),
        Some(TOKEN),
        Some(key),
        Some(json!({"input": input, "metadata": {"scenario":"mvp"}})),
    )
    .await;
    let Some(turn_id) = body["data"]["inbox_message"]["turn_id"].as_str() else {
        return (status, body);
    };
    let (turn_status, turn_body) = request_json(
        state,
        "GET",
        &format!("/external/v1/sessions/{session_id}/turns/{turn_id}"),
        Some(TOKEN),
        None,
        None,
    )
    .await;
    assert_eq!(turn_status, StatusCode::OK);
    (
        status,
        json!({ "data": { "turn": turn_body["data"]["turn"].clone() } }),
    )
}

#[tokio::test]
async fn external_api_has_stable_error_semantics_and_idempotency() {
    let _scope = GenericClientTestScope::new().await;
    let state = test_state("mvp_errors").await;

    let (unauth_status, unauth_body) = request_json(
        state.clone(),
        "GET",
        "/external/v1/sessions",
        None,
        None,
        None,
    )
    .await;
    assert_eq!(unauth_status, StatusCode::UNAUTHORIZED);
    assert_eq!(unauth_body["error"]["code"], "authentication_failed");

    let (invalid_status, invalid_body) = request_json(
        state.clone(),
        "POST",
        "/external/v1/sessions",
        Some(TOKEN),
        None,
        Some(json!({"client_type":"unsupported"})),
    )
    .await;
    assert_eq!(invalid_status, StatusCode::BAD_REQUEST);
    assert_eq!(invalid_body["error"]["code"], "invalid_request");

    let (not_found_status, not_found_body) = request_json(
        state.clone(),
        "GET",
        "/external/v1/sessions/sess_missing",
        Some(TOKEN),
        None,
        None,
    )
    .await;
    assert_eq!(not_found_status, StatusCode::NOT_FOUND);
    assert_eq!(not_found_body["error"]["code"], "not_found");

    let (first_create_status, first_create_body) =
        create_session_with_key(state.clone(), "mvp-idempotent-session").await;
    let (second_create_status, second_create_body) =
        create_session_with_key(state.clone(), "mvp-idempotent-session").await;
    assert_eq!(first_create_status, StatusCode::CREATED);
    assert_eq!(second_create_status, StatusCode::OK);
    let session_id = first_create_body["data"]["session"]["session_id"]
        .as_str()
        .expect("session id")
        .to_string();
    assert_eq!(
        second_create_body["data"]["session"]["session_id"],
        session_id
    );

    let (first_turn_status, first_turn_body) = submit_turn_with_key(
        state.clone(),
        &session_id,
        "mvp-idempotent-turn",
        "one turn",
    )
    .await;
    let (second_turn_status, second_turn_body) = submit_turn_with_key(
        state.clone(),
        &session_id,
        "mvp-idempotent-turn",
        "one turn",
    )
    .await;
    assert_eq!(first_turn_status, StatusCode::CREATED);
    assert_eq!(second_turn_status, StatusCode::OK);
    let turn_id = first_turn_body["data"]["turn"]["turn_id"]
        .as_str()
        .expect("turn id")
        .to_string();
    assert_eq!(second_turn_body["data"]["turn"]["turn_id"], turn_id);

    let (queued_status, queued_body) = request_json(
        state.clone(),
        "POST",
        &format!("/external/v1/sessions/{session_id}/inbox/messages"),
        Some(TOKEN),
        Some("mvp-conflicting-turn"),
        Some(json!({"input": "second active turn", "metadata": {"scenario":"mvp"}})),
    )
    .await;
    assert_eq!(queued_status, StatusCode::CREATED);
    assert_eq!(queued_body["data"]["inbox_message"]["state"], "pending");
    assert_eq!(queued_body["data"]["inbox_message"]["turn_id"], Value::Null);

    let (capability_status, capability_body) = request_json(
        state,
        "POST",
        &format!("/external/v1/sessions/{session_id}/turns/{turn_id}/interrupt"),
        Some(TOKEN),
        Some("mvp-capability"),
        None,
    )
    .await;
    assert_eq!(capability_status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(capability_body["error"]["code"], "capability_unavailable");
}
