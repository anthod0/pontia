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
    let db_path = dir.path().join("m5.db");
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

async fn create_session(state: AppState) -> String {
    let (status, body) = post_json(
        state,
        "/external/v1/sessions",
        Some(TOKEN),
        None,
        json!({"client_type":"generic"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    body["data"]["session"]["session_id"]
        .as_str()
        .expect("session id")
        .to_string()
}

async fn post_internal_event(state: AppState, body: Value) -> (StatusCode, Value) {
    let response = http::router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/internal/v1/events")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
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

fn event_body(event_id: &str, event_type: &str, session_id: &str, turn_id: &str) -> Value {
    json!({
        "event_id": event_id,
        "session_id": session_id,
        "turn_id": turn_id,
        "source": "agent_adapter",
        "client_type": "generic",
        "type": event_type,
        "time": "2026-04-25T12:00:00Z",
        "seq": 10,
        "payload": {}
    })
}

#[tokio::test]
async fn submit_turn_to_idle_session_creates_queued_turn_and_events() {
    let state = test_state().await;
    let session_id = create_session(state.clone()).await;

    let (status, body) = post_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/turns"),
        Some(TOKEN),
        None,
        json!({"input":"continue work","metadata":{"source":"m5"}}),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["error"], Value::Null);
    let turn = &body["data"]["turn"];
    let turn_id = turn["turn_id"].as_str().expect("turn id");
    assert!(turn_id.starts_with("turn_"));
    assert_eq!(turn["session_id"], session_id);
    assert_eq!(turn["state"], "queued");
    assert_eq!(turn["input"]["summary"], "continue work");
    assert_eq!(turn["metadata"]["source"], "m5");

    let (session_status, session_body) = get(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}"),
    )
    .await;
    assert_eq!(session_status, StatusCode::OK);
    assert_eq!(session_body["data"]["session"]["state"], "idle");
    assert_eq!(session_body["data"]["session"]["current_turn_id"], turn_id);

    let (events_status, events_body) = get(
        state,
        &format!("/external/v1/sessions/{session_id}/turns/{turn_id}/events"),
    )
    .await;
    assert_eq!(events_status, StatusCode::OK);
    let types: Vec<&str> = events_body["data"]["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|event| event["type"].as_str().unwrap())
        .collect();
    assert_eq!(types, vec!["turn.created", "turn.queued"]);
}

#[tokio::test]
async fn submit_turn_is_idempotent_with_same_key() {
    let state = test_state().await;
    let session_id = create_session(state.clone()).await;
    let uri = format!("/external/v1/sessions/{session_id}/turns");
    let request = json!({"input":"only once"});

    let first = post_json(
        state.clone(),
        &uri,
        Some(TOKEN),
        Some("submit-once"),
        request.clone(),
    )
    .await;
    let second = post_json(
        state.clone(),
        &uri,
        Some(TOKEN),
        Some("submit-once"),
        request,
    )
    .await;

    assert_eq!(first.0, StatusCode::CREATED);
    assert_eq!(second.0, StatusCode::OK);
    assert_eq!(second.1["data"], first.1["data"]);

    let turn_id = first.1["data"]["turn"]["turn_id"].as_str().unwrap();
    let (events_status, events_body) = get(
        state,
        &format!("/external/v1/sessions/{session_id}/turns/{turn_id}/events"),
    )
    .await;
    assert_eq!(events_status, StatusCode::OK);
    assert_eq!(events_body["data"]["events"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn submit_turn_rejects_busy_session_and_existing_active_turn() {
    let state = test_state().await;
    let session_id = create_session(state.clone()).await;
    let uri = format!("/external/v1/sessions/{session_id}/turns");

    let first = post_json(
        state.clone(),
        &uri,
        Some(TOKEN),
        None,
        json!({"input":"first"}),
    )
    .await;
    assert_eq!(first.0, StatusCode::CREATED);

    let second = post_json(state, &uri, Some(TOKEN), None, json!({"input":"second"})).await;

    assert_eq!(second.0, StatusCode::CONFLICT);
    assert_eq!(second.1["error"]["code"], "state_conflict");
}

#[tokio::test]
async fn interrupted_session_can_accept_next_turn() {
    let state = test_state().await;
    let session_id = create_session(state.clone()).await;
    let uri = format!("/external/v1/sessions/{session_id}/turns");
    let first = post_json(
        state.clone(),
        &uri,
        Some(TOKEN),
        None,
        json!({"input":"first"}),
    )
    .await;
    let first_turn_id = first.1["data"]["turn"]["turn_id"].as_str().unwrap();

    let (interrupt_status, _) = post_internal_event(
        state.clone(),
        event_body(
            "evt_m5_interrupted",
            "turn.interrupted",
            &session_id,
            first_turn_id,
        ),
    )
    .await;
    assert_eq!(interrupt_status, StatusCode::OK);

    let (status, body) = post_json(
        state,
        &uri,
        Some(TOKEN),
        None,
        json!({"input":"after interrupt"}),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["data"]["turn"]["state"], "queued");
}

#[tokio::test]
async fn terminal_or_starting_sessions_cannot_accept_turns() {
    let state = test_state().await;
    for (session_id, event_type) in [
        ("sess_m5_starting", "session.starting"),
        ("sess_m5_exited", "session.exited"),
        ("sess_m5_error", "session.error"),
    ] {
        let event = json!({
            "event_id": format!("evt_{session_id}"),
            "session_id": session_id,
            "turn_id": null,
            "source": "agent_adapter",
            "client_type": "generic",
            "type": event_type,
            "time": "2026-04-25T12:00:00Z",
            "seq": 1,
            "payload": {}
        });
        let (seed_status, _) = post_internal_event(state.clone(), event).await;
        assert_eq!(seed_status, StatusCode::OK);

        let (status, body) = post_json(
            state.clone(),
            &format!("/external/v1/sessions/{session_id}/turns"),
            Some(TOKEN),
            None,
            json!({"input":"not allowed"}),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(body["error"]["code"], "state_conflict");
    }
}

#[tokio::test]
async fn internal_events_advance_submitted_turn_and_session_projection() {
    let state = test_state().await;
    let session_id = create_session(state.clone()).await;
    let submit = post_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/turns"),
        Some(TOKEN),
        None,
        json!({"input":"run"}),
    )
    .await;
    let turn_id = submit.1["data"]["turn"]["turn_id"].as_str().unwrap();

    let (started_status, _) = post_internal_event(
        state.clone(),
        event_body("evt_m5_started", "turn.started", &session_id, turn_id),
    )
    .await;
    assert_eq!(started_status, StatusCode::OK);
    let (busy_status, busy_body) = get(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}"),
    )
    .await;
    assert_eq!(busy_status, StatusCode::OK);
    assert_eq!(busy_body["data"]["session"]["state"], "busy");

    let (completed_status, _) = post_internal_event(
        state.clone(),
        event_body("evt_m5_completed", "turn.completed", &session_id, turn_id),
    )
    .await;
    assert_eq!(completed_status, StatusCode::OK);

    let (turn_status, turn_body) = get(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/turns/{turn_id}"),
    )
    .await;
    assert_eq!(turn_status, StatusCode::OK);
    assert_eq!(turn_body["data"]["turn"]["state"], "completed");

    let (session_status, session_body) =
        get(state, &format!("/external/v1/sessions/{session_id}")).await;
    assert_eq!(session_status, StatusCode::OK);
    assert_eq!(session_body["data"]["session"]["state"], "idle");
    assert_eq!(
        session_body["data"]["session"]["current_turn_id"],
        Value::Null
    );
}
