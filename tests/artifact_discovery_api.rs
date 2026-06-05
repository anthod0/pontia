use std::{fs, path::Path};

use axum::{
    body::Body,
    http::{Method, Request, StatusCode, header},
};
use http_body_util::BodyExt;
use pilotfy::{
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
        graph: Default::default(),
        workspace_browser: Default::default(),
        dashboard: pilotfy::transport::http::dashboard::ResolvedDashboard::local_default(),
        shutdown: Default::default(),
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

async fn seed_idle_session(state: &AppState, workspace: &Path) {
    let service = EventIngestService::new(state.db.clone());
    service
        .ingest_event(event(
            "evt_m2_session_created",
            EventType::SessionCreated,
            "sess_m2_1",
            None,
            json!({}),
        ))
        .await
        .unwrap();
    service
        .ingest_event(event(
            "evt_m2_session_ready",
            EventType::SessionReady,
            "sess_m2_1",
            None,
            json!({}),
        ))
        .await
        .unwrap();
    sqlx::query("UPDATE sessions SET workspace_ref = ? WHERE session_id = ?")
        .bind(workspace.display().to_string())
        .bind("sess_m2_1")
        .execute(&state.db)
        .await
        .unwrap();
}

async fn request(
    state: AppState,
    method: Method,
    uri: &str,
    token: Option<&str>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
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
    let json: Value = serde_json::from_slice(&body).expect("json body");
    (status, json)
}

#[tokio::test]
async fn discovers_workspace_files_and_exposes_metadata_preview_and_content() {
    let state = test_state("m2_discover").await;
    let workspace = tempfile::tempdir().expect("workspace");
    fs::create_dir_all(workspace.path().join("reports")).expect("mkdir");
    fs::write(
        workspace.path().join("reports/summary.md"),
        "# Summary\nhello from pi\n",
    )
    .expect("write artifact");
    seed_idle_session(&state, workspace.path()).await;

    let (status, body) = request(
        state.clone(),
        Method::POST,
        "/external/v1/sessions/sess_m2_1/artifacts/discover",
        Some(TOKEN),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let artifacts = body["data"]["artifacts"].as_array().expect("artifacts");
    assert_eq!(artifacts.len(), 1);
    let artifact = &artifacts[0];
    assert_eq!(artifact["session_id"], "sess_m2_1");
    assert_eq!(artifact["turn_id"], Value::Null);
    assert_eq!(artifact["kind"], "markdown");
    assert_eq!(artifact["name"], "reports/summary.md");
    assert_eq!(artifact["size_bytes"], 24);
    assert_eq!(artifact["preview"], "# Summary\nhello from pi\n");
    assert_eq!(artifact["metadata"]["relative_path"], "reports/summary.md");
    assert!(artifact["metadata"]["modified_at"].as_str().is_some());
    assert!(
        artifact["metadata"]["content_fingerprint"]
            .as_str()
            .is_some()
    );
    assert!(artifact["metadata"].get("source_ref").is_none());

    let artifact_id = artifact["artifact_id"].as_str().expect("artifact_id");
    let (content_status, content_body, _content_type) = request_bytes(
        state,
        &format!("/external/v1/artifacts/{artifact_id}/content"),
        Some(TOKEN),
    )
    .await;
    assert_eq!(content_status, StatusCode::OK);
    assert_eq!(content_body, b"# Summary\nhello from pi\n");
}

#[tokio::test]
async fn discovery_stays_inside_workspace_and_does_not_follow_escape_symlinks() {
    let state = test_state("m2_sandbox").await;
    let workspace = tempfile::tempdir().expect("workspace");
    let outside = tempfile::tempdir().expect("outside");
    fs::write(workspace.path().join("safe.log"), "safe\n").expect("write safe");
    fs::write(outside.path().join("secret.log"), "secret\n").expect("write secret");
    #[cfg(unix)]
    std::os::unix::fs::symlink(
        outside.path().join("secret.log"),
        workspace.path().join("secret.log"),
    )
    .expect("symlink");
    seed_idle_session(&state, workspace.path()).await;

    let (status, body) = request(
        state,
        Method::POST,
        "/external/v1/sessions/sess_m2_1/artifacts/discover",
        Some(TOKEN),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let artifacts = body["data"]["artifacts"].as_array().expect("artifacts");
    assert_eq!(artifacts.len(), 1);
    assert_eq!(artifacts[0]["name"], "safe.log");
}

#[tokio::test]
async fn discovery_does_not_change_session_state_or_events() {
    let state = test_state("m2_no_domain_transition").await;
    let workspace = tempfile::tempdir().expect("workspace");
    fs::write(workspace.path().join("result.txt"), "result\n").expect("write");
    seed_idle_session(&state, workspace.path()).await;

    let (_, before) = request(
        state.clone(),
        Method::GET,
        "/external/v1/sessions/sess_m2_1",
        Some(TOKEN),
    )
    .await;
    let (_, events_before) = request(
        state.clone(),
        Method::GET,
        "/external/v1/sessions/sess_m2_1/events",
        Some(TOKEN),
    )
    .await;

    let (status, _) = request(
        state.clone(),
        Method::POST,
        "/external/v1/sessions/sess_m2_1/artifacts/discover",
        Some(TOKEN),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (_, after) = request(
        state.clone(),
        Method::GET,
        "/external/v1/sessions/sess_m2_1",
        Some(TOKEN),
    )
    .await;
    let (_, events_after) = request(
        state,
        Method::GET,
        "/external/v1/sessions/sess_m2_1/events",
        Some(TOKEN),
    )
    .await;

    assert_eq!(
        before["data"]["session"]["state"],
        after["data"]["session"]["state"]
    );
    assert_eq!(
        events_before["data"]["events"].as_array().unwrap().len(),
        events_after["data"]["events"].as_array().unwrap().len()
    );
}

async fn request_bytes(
    state: AppState,
    uri: &str,
    token: Option<&str>,
) -> (StatusCode, Vec<u8>, String) {
    let mut builder = Request::builder().method(Method::GET).uri(uri);
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
