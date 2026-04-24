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
    let db_path = dir.path().join("m4.db");
    let _kept_dir = dir.keep();
    let database_url = format!("sqlite://{}", db_path.display());
    let db = connect_sqlite(&database_url).await.expect("connect");
    run_migrations(&db).await.expect("migrate");
    AppState {
        db,
        external_api_token: Some(TOKEN.to_string()),
    }
}

async fn post_json(
    state: AppState,
    uri: &str,
    token: Option<&str>,
    idempotency_key: Option<&str>,
    body: Value,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method("POST")
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(token) = token {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
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

async fn get(state: AppState, uri: &str) -> (StatusCode, Value) {
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

#[tokio::test]
async fn create_session_rejects_unauthenticated_requests() {
    let state = test_state().await;

    let (status, body) = post_json(
        state,
        "/external/v1/sessions",
        None,
        None,
        json!({"client_type":"generic"}),
    )
    .await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["data"], Value::Null);
    assert_eq!(body["error"]["code"], "authentication_failed");
}

#[tokio::test]
async fn create_session_emits_lifecycle_events_and_returns_idle_session_with_capabilities() {
    let state = test_state().await;

    let (status, body) = post_json(
        state.clone(),
        "/external/v1/sessions",
        Some(TOKEN),
        None,
        json!({
            "client_type":"generic",
            "workspace":"/tmp/workspace",
            "metadata":{"purpose":"m4"}
        }),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["error"], Value::Null);
    let session = &body["data"]["session"];
    let session_id = session["session_id"].as_str().expect("session id");
    assert!(session_id.starts_with("sess_"));
    assert_eq!(session["client_type"], "generic");
    assert_eq!(session["state"], "idle");
    assert_eq!(session["workspace"], "/tmp/workspace");
    assert_eq!(session["metadata"]["purpose"], "m4");
    assert_eq!(session["capabilities"]["accept_task"], true);
    assert_eq!(session["capabilities"]["interrupt"], false);
    assert_eq!(body["data"]["initial_turn"], Value::Null);

    let (events_status, events_body) = get(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/events"),
    )
    .await;
    assert_eq!(events_status, StatusCode::OK);
    let types: Vec<&str> = events_body["data"]["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|event| event["type"].as_str().unwrap())
        .collect();
    assert_eq!(
        types,
        vec![
            "session.created",
            "session.starting",
            "session.started",
            "session.ready"
        ]
    );

    let (get_status, get_body) = get(state, &format!("/external/v1/sessions/{session_id}")).await;
    assert_eq!(get_status, StatusCode::OK);
    assert_eq!(get_body["data"]["session"]["state"], "idle");
    assert_eq!(
        get_body["data"]["session"]["capabilities"]["accept_task"],
        true
    );
}

#[tokio::test]
async fn create_session_with_initial_task_creates_queued_initial_turn() {
    let state = test_state().await;

    let (status, body) = post_json(
        state.clone(),
        "/external/v1/sessions",
        Some(TOKEN),
        None,
        json!({
            "client_type":"generic",
            "initial_task":{"input":"do the first thing","metadata":{"priority":"high"}}
        }),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    let session_id = body["data"]["session"]["session_id"].as_str().unwrap();
    let initial_turn = &body["data"]["initial_turn"];
    let turn_id = initial_turn["turn_id"].as_str().expect("turn id");
    assert!(turn_id.starts_with("turn_"));
    assert_eq!(initial_turn["session_id"], session_id);
    assert_eq!(initial_turn["state"], "queued");
    assert_eq!(initial_turn["input"]["summary"], "do the first thing");
    assert_eq!(body["data"]["session"]["current_turn_id"], turn_id);

    let (turn_status, turn_body) = get(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/turns/{turn_id}"),
    )
    .await;
    assert_eq!(turn_status, StatusCode::OK);
    assert_eq!(turn_body["data"]["turn"]["state"], "queued");
    assert_eq!(
        turn_body["data"]["turn"]["input"]["summary"],
        "do the first thing"
    );
    assert_eq!(turn_body["data"]["turn"]["metadata"]["priority"], "high");
}

#[tokio::test]
async fn create_session_is_idempotent_when_idempotency_key_is_retried() {
    let state = test_state().await;
    let body = json!({
        "client_type":"generic",
        "initial_task":{"input":"only once"}
    });

    let first = post_json(
        state.clone(),
        "/external/v1/sessions",
        Some(TOKEN),
        Some("create-session-once"),
        body.clone(),
    )
    .await;
    let second = post_json(
        state.clone(),
        "/external/v1/sessions",
        Some(TOKEN),
        Some("create-session-once"),
        body,
    )
    .await;

    assert_eq!(first.0, StatusCode::CREATED);
    assert_eq!(second.0, StatusCode::OK);
    assert_eq!(second.1["data"], first.1["data"]);

    let session_id = first.1["data"]["session"]["session_id"].as_str().unwrap();
    let (events_status, events_body) =
        get(state, &format!("/external/v1/sessions/{session_id}/events")).await;
    assert_eq!(events_status, StatusCode::OK);
    assert_eq!(events_body["data"]["events"].as_array().unwrap().len(), 6);
}
