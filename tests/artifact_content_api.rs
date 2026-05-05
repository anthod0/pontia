use std::{fs, path::Path};

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

async fn test_state(name: &str) -> AppState {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join(format!("{name}.db"));
    let _kept_dir = dir.keep();
    let database_url = format!("sqlite://{}", db_path.display());
    let db = connect_sqlite(&database_url).await.expect("connect");
    run_migrations(&db).await.expect("migrate");
    AppState {
        db,
        external_api_token: Some(TOKEN.to_string()),
        planner: Default::default(),
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

async fn seed_idle_session(state: &AppState) {
    let service = EventIngestService::new(state.db.clone());
    service
        .ingest_event(event(
            "evt_m7_session_created",
            EventType::SessionCreated,
            "sess_m7_1",
            None,
            json!({}),
        ))
        .await
        .unwrap();
    service
        .ingest_event(event(
            "evt_m7_session_ready",
            EventType::SessionReady,
            "sess_m7_1",
            None,
            json!({}),
        ))
        .await
        .unwrap();
}

async fn insert_artifact(state: &AppState, artifact_id: &str, source_ref: &str, size: i64) {
    sqlx::query(
        r#"INSERT INTO artifacts
           (artifact_id, session_id, turn_id, kind, name, source_ref, size_bytes, metadata)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind(artifact_id)
    .bind("sess_m7_1")
    .bind(Option::<String>::None)
    .bind("log")
    .bind("agent.log")
    .bind(source_ref)
    .bind(size)
    .bind(json!({"preview":"hello", "note":"public"}).to_string())
    .execute(&state.db)
    .await
    .unwrap();
}

async fn request(state: AppState, uri: &str, token: Option<&str>) -> (StatusCode, Vec<u8>, String) {
    let mut builder = Request::builder().method("GET").uri(uri);
    if let Some(token) = token {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }

    let response = http::router(state)
        .oneshot(builder.body(Body::empty()).expect("request"))
        .await
        .expect("response");

    let status = response.status();
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let body = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes()
        .to_vec();
    (status, body, content_type)
}

fn file_url(path: &Path) -> String {
    format!("file://{}", path.display())
}

#[tokio::test]
async fn external_api_reads_registered_artifact_content() {
    let state = test_state("m7_content").await;
    seed_idle_session(&state).await;
    let dir = tempfile::tempdir().expect("artifact dir");
    let artifact_path = dir.path().join("agent.log");
    fs::write(&artifact_path, "hello artifact\n").expect("write artifact");
    insert_artifact(&state, "art_m7_1", &file_url(&artifact_path), 15).await;

    let (status, body, content_type) = request(
        state,
        "/external/v1/artifacts/art_m7_1/content",
        Some(TOKEN),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, b"hello artifact\n");
    assert!(content_type.starts_with("application/octet-stream"));
}

#[tokio::test]
async fn artifact_content_requires_existing_artifact_index_entry() {
    let state = test_state("m7_missing").await;

    let (status, body, _content_type) = request(
        state,
        "/external/v1/artifacts/art_missing/content",
        Some(TOKEN),
    )
    .await;
    let json: Value = serde_json::from_slice(&body).expect("json error body");

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(json["error"]["code"], "not_found");
}

#[tokio::test]
async fn artifact_content_rejects_unregistered_source_schemes() {
    let state = test_state("m7_reject_scheme").await;
    seed_idle_session(&state).await;
    insert_artifact(&state, "art_m7_bad", "/etc/passwd", 100).await;

    let (status, body, _content_type) = request(
        state,
        "/external/v1/artifacts/art_m7_bad/content",
        Some(TOKEN),
    )
    .await;
    let json: Value = serde_json::from_slice(&body).expect("json error body");

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["error"]["code"], "invalid_request");
}

#[tokio::test]
async fn artifact_metadata_and_content_size_are_consistent() {
    let state = test_state("m7_size").await;
    seed_idle_session(&state).await;
    let dir = tempfile::tempdir().expect("artifact dir");
    let artifact_path = dir.path().join("agent.log");
    fs::write(&artifact_path, "actual content").expect("write artifact");
    insert_artifact(&state, "art_m7_size", &file_url(&artifact_path), 999).await;

    let (status, body, _content_type) = request(
        state,
        "/external/v1/artifacts/art_m7_size/content",
        Some(TOKEN),
    )
    .await;
    let json: Value = serde_json::from_slice(&body).expect("json error body");

    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(json["error"]["code"], "state_conflict");
}

#[tokio::test]
async fn large_artifact_content_returns_explicit_error_instead_of_loading_bytes() {
    let state = test_state("m2_large_content").await;
    seed_idle_session(&state).await;
    let dir = tempfile::tempdir().expect("artifact dir");
    let artifact_path = dir.path().join("large.log");
    let large_content = vec![b'x'; 1024 * 1024 + 1];
    fs::write(&artifact_path, &large_content).expect("write artifact");
    insert_artifact(
        &state,
        "art_m2_large",
        &file_url(&artifact_path),
        large_content.len() as i64,
    )
    .await;

    let (status, body, _content_type) = request(
        state,
        "/external/v1/artifacts/art_m2_large/content",
        Some(TOKEN),
    )
    .await;
    let json: Value = serde_json::from_slice(&body).expect("json error body");

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["error"]["code"], "invalid_request");
    assert!(
        json["error"]["message"]
            .as_str()
            .unwrap()
            .contains("too large")
    );
}

#[tokio::test]
async fn empty_artifact_source_list_does_not_break_session_turn_flow() {
    let state = test_state("m7_empty_list").await;
    seed_idle_session(&state).await;

    let (status, body, _content_type) = request(
        state,
        "/external/v1/sessions/sess_m7_1/artifacts",
        Some(TOKEN),
    )
    .await;
    let json: Value = serde_json::from_slice(&body).expect("json body");

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["data"]["artifacts"].as_array().unwrap().len(), 0);
}
