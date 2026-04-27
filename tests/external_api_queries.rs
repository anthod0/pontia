use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use llmparty::{
    application::{AppState, EventIngestService},
    domain::{DomainEvent, EventSource, EventType},
    storage::sqlite::{connect_sqlite, run_migrations},
    transport::http,
};
use serde_json::{Value, json};
use tower::ServiceExt;

const TOKEN: &str = "test-token";

async fn test_state() -> AppState {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("m3.db");
    let _kept_dir = dir.keep();
    let database_url = format!("sqlite://{}", db_path.display());
    let db = connect_sqlite(&database_url).await.expect("connect");
    run_migrations(&db).await.expect("migrate");
    AppState {
        db,
        external_api_token: Some(TOKEN.to_string()),
    }
}

fn event(
    event_id: &str,
    event_type: EventType,
    session_id: &str,
    turn_id: Option<&str>,
    payload: Value,
) -> DomainEvent {
    DomainEvent::new(
        event_id.to_string(),
        session_id.to_string(),
        turn_id.map(str::to_string),
        EventSource::AgentAdapter,
        "generic".to_string(),
        event_type,
        payload,
    )
}

async fn seed_session_turn(state: &AppState) {
    let service = EventIngestService::new(state.db.clone());
    service
        .ingest_event(event(
            "evt_m3_1",
            EventType::SessionCreated,
            "sess_m3_1",
            None,
            json!({"metadata":{"purpose":"test"}}),
        ))
        .await
        .unwrap();
    service
        .ingest_event(event(
            "evt_m3_2",
            EventType::SessionReady,
            "sess_m3_1",
            None,
            json!({}),
        ))
        .await
        .unwrap();
    service
        .ingest_event(event(
            "evt_m3_3",
            EventType::TurnStarted,
            "sess_m3_1",
            Some("turn_m3_1"),
            json!({"input":{"summary":"do work"}}),
        ))
        .await
        .unwrap();
    service
        .ingest_event(event(
            "evt_m3_4",
            EventType::TurnCompleted,
            "sess_m3_1",
            Some("turn_m3_1"),
            json!({"output":{"summary":"done"}}),
        ))
        .await
        .unwrap();
    service
        .ingest_event(event(
            "evt_m3_5",
            EventType::TurnFailed,
            "sess_m3_1",
            Some("turn_m3_1"),
            json!({"failure":{"message":"ignored after completion"}}),
        ))
        .await
        .unwrap();
    service
        .ingest_event(event(
            "evt_m3_0",
            EventType::TurnCompleted,
            "sess_m3_1",
            Some("turn_m3_1"),
            json!({"output":{"summary":"ignored late completion"}}),
        ))
        .await
        .unwrap();
    sqlx::query(
        "UPDATE events SET created_at = '2026-04-24T12:00:00.000Z' WHERE event_id IN ('evt_m3_4', 'evt_m3_0')",
    )
    .execute(&state.db)
    .await
    .unwrap();

    sqlx::query(
        r#"INSERT INTO artifacts
           (artifact_id, session_id, turn_id, kind, name, source_ref, size_bytes, metadata)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind("art_m3_1")
    .bind("sess_m3_1")
    .bind("turn_m3_1")
    .bind("log")
    .bind("agent.log")
    .bind("registered://agent.log")
    .bind(12_i64)
    .bind(json!({"preview":"hello world", "source_ref":"internal-path"}).to_string())
    .execute(&state.db)
    .await
    .unwrap();
}

async fn get(state: AppState, uri: &str, token: Option<&str>) -> (StatusCode, Value) {
    let mut builder = Request::builder().method("GET").uri(uri);
    if let Some(token) = token {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }

    let response = http::router(state)
        .oneshot(builder.body(Body::empty()).expect("request"))
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
async fn external_api_rejects_missing_or_wrong_bearer_token() {
    let state = test_state().await;

    let missing = get(state.clone(), "/external/v1/sessions", None).await;
    let wrong = get(state, "/external/v1/sessions", Some("wrong-token")).await;

    assert_eq!(missing.0, StatusCode::UNAUTHORIZED);
    assert_eq!(missing.1["data"], Value::Null);
    assert_eq!(missing.1["error"]["code"], "authentication_failed");
    assert_eq!(wrong.0, StatusCode::UNAUTHORIZED);
    assert_eq!(wrong.1["error"]["code"], "authentication_failed");
}

#[tokio::test]
async fn external_api_lists_and_gets_session_views() {
    let state = test_state().await;
    seed_session_turn(&state).await;

    let (list_status, list_body) = get(state.clone(), "/external/v1/sessions", Some(TOKEN)).await;
    let (get_status, get_body) = get(state, "/external/v1/sessions/sess_m3_1", Some(TOKEN)).await;

    assert_eq!(list_status, StatusCode::OK);
    assert_eq!(list_body["error"], Value::Null);
    assert_eq!(list_body["data"]["sessions"].as_array().unwrap().len(), 1);
    assert_eq!(list_body["data"]["sessions"][0]["session_id"], "sess_m3_1");
    assert_eq!(list_body["data"]["sessions"][0]["state"], "idle");
    assert_eq!(list_body["data"]["sessions"][0]["client_type"], "generic");
    assert!(list_body["data"]["sessions"][0]["capabilities"].is_object());

    assert_eq!(get_status, StatusCode::OK);
    assert_eq!(get_body["data"]["session"]["session_id"], "sess_m3_1");
    assert_eq!(get_body["data"]["session"]["current_turn_id"], Value::Null);
}

#[tokio::test]
async fn external_api_lists_and_gets_turn_views() {
    let state = test_state().await;
    seed_session_turn(&state).await;

    let (list_status, list_body) = get(
        state.clone(),
        "/external/v1/sessions/sess_m3_1/turns",
        Some(TOKEN),
    )
    .await;
    let (get_status, get_body) = get(
        state,
        "/external/v1/sessions/sess_m3_1/turns/turn_m3_1",
        Some(TOKEN),
    )
    .await;

    assert_eq!(list_status, StatusCode::OK);
    assert_eq!(list_body["data"]["turns"].as_array().unwrap().len(), 1);
    assert_eq!(list_body["data"]["turns"][0]["turn_id"], "turn_m3_1");
    assert_eq!(list_body["data"]["turns"][0]["state"], "completed");

    assert_eq!(get_status, StatusCode::OK);
    assert_eq!(get_body["data"]["turn"]["turn_id"], "turn_m3_1");
    assert_eq!(get_body["data"]["turn"]["session_id"], "sess_m3_1");
    assert_eq!(get_body["data"]["turn"]["input"]["summary"], "do work");
    assert_eq!(get_body["data"]["turn"]["output"]["summary"], "done");
    assert_eq!(get_body["data"]["turn"]["failure"], Value::Null);
    assert!(get_body["data"]["turn"]["started_at"].is_string());
    assert!(get_body["data"]["turn"]["completed_at"].is_string());
}

#[tokio::test]
async fn external_api_lists_session_and_turn_events() {
    let state = test_state().await;
    seed_session_turn(&state).await;

    let (session_status, session_body) = get(
        state.clone(),
        "/external/v1/sessions/sess_m3_1/events",
        Some(TOKEN),
    )
    .await;
    let (turn_status, turn_body) = get(
        state,
        "/external/v1/sessions/sess_m3_1/turns/turn_m3_1/events",
        Some(TOKEN),
    )
    .await;

    assert_eq!(session_status, StatusCode::OK);
    assert_eq!(session_body["data"]["events"].as_array().unwrap().len(), 6);
    assert_eq!(session_body["data"]["events"][0]["type"], "session.created");
    assert_eq!(session_body["data"]["events"][0]["source"], "agent_adapter");

    assert_eq!(turn_status, StatusCode::OK);
    assert_eq!(turn_body["data"]["events"].as_array().unwrap().len(), 4);
    assert_eq!(turn_body["data"]["events"][0]["turn_id"], "turn_m3_1");
}

#[tokio::test]
async fn external_api_lists_and_gets_artifact_metadata_without_source_ref() {
    let state = test_state().await;
    seed_session_turn(&state).await;

    let (list_status, list_body) = get(
        state.clone(),
        "/external/v1/sessions/sess_m3_1/artifacts",
        Some(TOKEN),
    )
    .await;
    let (get_status, get_body) = get(state, "/external/v1/artifacts/art_m3_1", Some(TOKEN)).await;

    assert_eq!(list_status, StatusCode::OK);
    assert_eq!(list_body["data"]["artifacts"].as_array().unwrap().len(), 1);
    assert_eq!(list_body["data"]["artifacts"][0]["artifact_id"], "art_m3_1");
    assert_eq!(list_body["data"]["artifacts"][0]["preview"], "hello world");
    assert!(
        list_body["data"]["artifacts"][0]
            .get("source_ref")
            .is_none()
    );

    assert_eq!(get_status, StatusCode::OK);
    assert_eq!(get_body["data"]["artifact"]["artifact_id"], "art_m3_1");
    assert!(get_body["data"]["artifact"].get("source_ref").is_none());
    assert!(
        get_body["data"]["artifact"]["metadata"]
            .get("source_ref")
            .is_none()
    );
}

#[tokio::test]
async fn external_api_returns_clear_not_found_errors() {
    let state = test_state().await;

    let session = get(
        state.clone(),
        "/external/v1/sessions/sess_missing",
        Some(TOKEN),
    )
    .await;
    let artifact = get(state, "/external/v1/artifacts/art_missing", Some(TOKEN)).await;

    assert_eq!(session.0, StatusCode::NOT_FOUND);
    assert_eq!(session.1["data"], Value::Null);
    assert_eq!(session.1["error"]["code"], "not_found");
    assert_eq!(artifact.0, StatusCode::NOT_FOUND);
    assert_eq!(artifact.1["error"]["code"], "not_found");
}
