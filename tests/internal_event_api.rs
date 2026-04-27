use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use llmparty::{
    application::{AppState, EventIngestService},
    domain::{SessionState, TurnState},
    storage::sqlite::{connect_sqlite, run_migrations},
    transport::http,
};
use serde_json::{Value, json};
use tower::ServiceExt;

async fn test_state() -> AppState {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("m2.db");
    let _kept_dir = dir.keep();
    let database_url = format!("sqlite://{}", db_path.display());
    let db = connect_sqlite(&database_url).await.expect("connect");
    run_migrations(&db).await.expect("migrate");
    AppState {
        db,
        external_api_token: None,
    }
}

fn event_body(
    event_id: &str,
    event_type: &str,
    session_id: &str,
    turn_id: Option<&str>,
    seq: i64,
) -> Value {
    json!({
        "event_id": event_id,
        "session_id": session_id,
        "turn_id": turn_id,
        "source": "agent_adapter",
        "client_type": "generic",
        "type": event_type,
        "time": "2026-04-24T12:00:00Z",
        "seq": seq,
        "payload": {}
    })
}

async fn post_event(state: AppState, body: Value) -> (StatusCode, Value) {
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

#[tokio::test]
async fn internal_event_api_accepts_session_event_and_updates_projection() {
    let state = test_state().await;

    let (status, body) = post_event(
        state.clone(),
        event_body("evt_m2_1", "session.created", "sess_m2_1", None, 1),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["accepted"], true);
    assert_eq!(body["duplicate"], false);
    assert_eq!(body["state_version"], 1);
    assert_eq!(body["warnings"], json!([]));

    let service = EventIngestService::new(state.db);
    let session = service
        .get_session("sess_m2_1")
        .await
        .expect("session query")
        .expect("session projection");
    assert_eq!(session.state, SessionState::Created);
}

#[tokio::test]
async fn internal_event_api_accepts_turn_events_and_updates_projection() {
    let state = test_state().await;

    post_event(
        state.clone(),
        event_body("evt_m2_2", "session.ready", "sess_m2_2", None, 1),
    )
    .await;
    let (status, body) = post_event(
        state.clone(),
        event_body(
            "evt_m2_3",
            "turn.started",
            "sess_m2_2",
            Some("turn_m2_1"),
            2,
        ),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["turn_id"], "turn_m2_1");
    assert_eq!(body["state_version"], 2);

    let service = EventIngestService::new(state.db);
    let session = service.get_session("sess_m2_2").await.unwrap().unwrap();
    let turn = service.get_turn("turn_m2_1").await.unwrap().unwrap();
    assert_eq!(session.state, SessionState::Busy);
    assert_eq!(turn.state, TurnState::Running);
}

#[tokio::test]
async fn internal_event_api_rejects_missing_required_schema_fields() {
    let state = test_state().await;
    let mut event = event_body(
        "evt_m2_missing",
        "session.created",
        "sess_m2_missing",
        None,
        1,
    );
    event.as_object_mut().unwrap().remove("event_id");

    let (status, body) = post_event(state, event).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "invalid_request");
}

#[tokio::test]
async fn internal_event_api_rejects_unknown_event_type() {
    let state = test_state().await;
    let (status, body) = post_event(
        state,
        event_body("evt_m2_4", "approval.requested", "sess_m2_3", None, 1),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "invalid_request");
}

#[tokio::test]
async fn internal_event_api_rejects_turn_event_without_turn_id() {
    let state = test_state().await;
    let (status, body) = post_event(
        state,
        event_body("evt_m2_5", "turn.completed", "sess_m2_4", None, 1),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "invalid_request");
}

#[tokio::test]
async fn internal_event_api_is_idempotent_for_duplicate_event_id() {
    let state = test_state().await;
    let event = event_body("evt_m2_same", "session.created", "sess_m2_5", None, 1);

    let first = post_event(state.clone(), event.clone()).await;
    let second = post_event(state.clone(), event).await;

    assert_eq!(first.0, StatusCode::OK);
    assert_eq!(second.0, StatusCode::OK);
    assert_eq!(second.1["accepted"], true);
    assert_eq!(second.1["duplicate"], true);
    assert_eq!(second.1["state_version"], 1);

    let service = EventIngestService::new(state.db);
    assert_eq!(service.list_events("sess_m2_5").await.unwrap().len(), 1);
}

#[tokio::test]
async fn internal_event_api_maps_domain_conflicts_to_conflict() {
    let state = test_state().await;

    post_event(
        state.clone(),
        event_body("evt_m2_6", "session.ready", "sess_m2_6", None, 1),
    )
    .await;
    post_event(
        state.clone(),
        event_body(
            "evt_m2_7",
            "turn.started",
            "sess_m2_6",
            Some("turn_m2_2"),
            2,
        ),
    )
    .await;
    let (status, body) = post_event(
        state,
        event_body(
            "evt_m2_8",
            "turn.started",
            "sess_m2_6",
            Some("turn_m2_3"),
            3,
        ),
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "state_conflict");
}

#[tokio::test]
async fn internal_event_api_rejects_large_payloads() {
    let state = test_state().await;
    let mut event = event_body("evt_m2_9", "turn.output", "sess_m2_7", Some("turn_m2_4"), 1);
    event["payload"] = json!({ "content": "x".repeat(70_000) });

    let (status, body) = post_event(state, event).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "invalid_request");
}

#[tokio::test]
async fn internal_event_api_accepts_sequence_gaps_with_warnings() {
    let state = test_state().await;

    post_event(
        state.clone(),
        event_body("evt_m2_10", "session.created", "sess_m2_8", None, 1),
    )
    .await;
    let (status, body) = post_event(
        state,
        event_body("evt_m2_11", "session.ready", "sess_m2_8", None, 3),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["accepted"], true);
    assert_eq!(body["warnings"].as_array().unwrap().len(), 1);
    assert!(
        body["warnings"][0]
            .as_str()
            .unwrap()
            .contains("sequence gap")
    );
}
