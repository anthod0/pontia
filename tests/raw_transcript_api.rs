use std::fs;

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use pontia::{
    application::{AgentBindingService, AppState, EventIngestService, UpsertAgentBindingRequest},
    domain::{DomainEvent, EventSource, EventType},
    storage::sqlite::{connect_sqlite, run_migrations},
    transport::http,
};
use serde_json::{Value, json};
use tempfile::tempdir;
use tokio::sync::Mutex;
use tower::ServiceExt;

const TOKEN: &str = "test-token";
static PI_AGENT_DIR_ENV_LOCK: Mutex<()> = Mutex::const_new(());

async fn test_state() -> AppState {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("raw-transcript-api.db");
    let _kept_dir = dir.keep();
    let database_url = format!("sqlite://{}", db_path.display());
    let db = connect_sqlite(&database_url).await.expect("connect");
    run_migrations(&db).await.expect("migrate");
    AppState::builder(db)
        .external_api_token(Some(TOKEN.to_string()))
        .build()
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

fn pi_session_dir(agent_dir: &std::path::Path, cwd: &std::path::Path) -> std::path::PathBuf {
    let safe = format!(
        "--{}--",
        cwd.to_string_lossy()
            .trim_start_matches('/')
            .replace(['/', '\\', ':'], "-")
    );
    agent_dir.join("sessions").join(safe)
}

async fn seed_session(state: &AppState, session_id: &str) {
    let service = EventIngestService::new(state.db());
    service
        .ingest_event(DomainEvent::new(
            format!("evt_{session_id}_created"),
            session_id.to_string(),
            None,
            EventSource::AgentAdapter,
            "pi".to_string(),
            EventType::SessionCreated,
            json!({}),
        ))
        .await
        .unwrap();
}

async fn seed_session_exited(state: &AppState, session_id: &str) {
    EventIngestService::new(state.db())
        .ingest_event(DomainEvent::new(
            format!("evt_{session_id}_exited"),
            session_id.to_string(),
            None,
            EventSource::RuntimeManager,
            "pi".to_string(),
            EventType::SessionExited,
            json!({}),
        ))
        .await
        .unwrap();
}

#[tokio::test]
async fn timeline_and_detail_external_api_read_pi_jsonl_fixture() {
    let _guard = PI_AGENT_DIR_ENV_LOCK.lock().await;
    let temp = tempdir().unwrap();
    let agent_dir = temp.path().join("agent");
    unsafe { std::env::set_var("PI_AGENT_DIR", &agent_dir) };

    let state = test_state().await;
    let session_id = "sess_raw_api";
    let session_key = "11111111-2222-3333-4444-555555555555";
    let cwd = temp.path().join("workspace");
    fs::create_dir_all(&cwd).unwrap();
    let cwd = cwd.canonicalize().unwrap();
    seed_session(&state, session_id).await;

    let session_dir = pi_session_dir(&agent_dir, &cwd);
    fs::create_dir_all(&session_dir).unwrap();
    fs::write(
        session_dir.join(format!("2026-06-09T00-00-00-000Z_{session_key}.jsonl")),
        concat!(
            "{\"type\":\"message\",\"id\":\"u1\",\"timestamp\":\"2026-06-09T00:00:01.000Z\",\"message\":{\"role\":\"user\",\"content\":\"hello timeline\"}}\n",
            "{\"type\":\"message\",\"id\":\"a1\",\"timestamp\":\"2026-06-09T00:00:02.000Z\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"answer body\"}]}}\n",
        ),
    )
    .unwrap();

    AgentBindingService::new(state.db())
        .upsert_binding(UpsertAgentBindingRequest {
            session_id: session_id.to_string(),
            client_type: "pi".to_string(),
            launch_cwd: cwd.to_string_lossy().to_string(),
            client_session_key: session_key.to_string(),
            metadata: json!({}),
        })
        .await
        .unwrap();

    let (status, body) = get_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/timeline?limit=1"),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["session_id"], session_id);
    assert_eq!(body["data"]["items"].as_array().unwrap().len(), 2);
    assert_eq!(body["data"]["items"][0]["kind"], "user");
    assert_eq!(
        body["data"]["items"][0]["content_preview"],
        "hello timeline"
    );
    assert_eq!(body["data"]["items"][1]["kind"], "assistant");
    assert_eq!(body["data"]["has_more"], false);
    assert_eq!(body["data"]["is_tail"], true);
    assert!(body["data"]["next_cursor"].is_null());
    assert!(
        body["data"]["source_id"]
            .as_str()
            .unwrap()
            .starts_with("pi:")
    );

    let detail_ref = body["data"]["items"][0]["content_ref"].as_str().unwrap();
    let (detail_status, detail_body) = get_json(
        state.clone(),
        &format!(
            "/external/v1/sessions/{session_id}/timeline/detail?ref={}",
            urlencoding_for_test(detail_ref)
        ),
    )
    .await;

    assert_eq!(detail_status, StatusCode::OK);
    assert_eq!(detail_body["data"]["content_ref"], detail_ref);
    assert_eq!(detail_body["data"]["content_type"], "application/json");
    assert!(
        detail_body["data"]["text"]
            .as_str()
            .unwrap()
            .contains("hello timeline")
    );

    let (cursor_status, cursor_body) = get_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/timeline?cursor=bad-cursor"),
    )
    .await;
    assert_eq!(cursor_status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(cursor_body["error"]["code"], "cursor_invalid");

    let (ref_status, ref_body) = get_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/timeline/detail?ref=bad-ref"),
    )
    .await;
    assert_eq!(ref_status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(ref_body["error"]["code"], "content_ref_invalid");

    unsafe { std::env::remove_var("PI_AGENT_DIR") };
}

#[tokio::test]
async fn timeline_external_api_returns_not_ready_when_raw_file_has_not_been_discovered() {
    let _guard = PI_AGENT_DIR_ENV_LOCK.lock().await;
    let temp = tempdir().unwrap();
    let agent_dir = temp.path().join("agent");
    unsafe { std::env::set_var("PI_AGENT_DIR", &agent_dir) };

    let state = test_state().await;
    let session_id = "sess_raw_pending_source";
    let cwd = temp.path().join("workspace");
    fs::create_dir_all(&cwd).unwrap();
    let cwd = cwd.canonicalize().unwrap();
    seed_session(&state, session_id).await;

    AgentBindingService::new(state.db())
        .upsert_binding(UpsertAgentBindingRequest {
            session_id: session_id.to_string(),
            client_type: "pi".to_string(),
            launch_cwd: cwd.to_string_lossy().to_string(),
            client_session_key: "missing-session-key".to_string(),
            metadata: json!({}),
        })
        .await
        .unwrap();

    let (status, body) = get_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/timeline"),
    )
    .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"]["code"], "not_ready");

    unsafe { std::env::remove_var("PI_AGENT_DIR") };
}

#[tokio::test]
async fn timeline_external_api_returns_source_unavailable_when_discovered_raw_file_disappears() {
    let _guard = PI_AGENT_DIR_ENV_LOCK.lock().await;
    let temp = tempdir().unwrap();
    let agent_dir = temp.path().join("agent");
    unsafe { std::env::set_var("PI_AGENT_DIR", &agent_dir) };

    let state = test_state().await;
    let session_id = "sess_raw_discovered_missing_source";
    let cwd = temp.path().join("workspace");
    fs::create_dir_all(&cwd).unwrap();
    let cwd = cwd.canonicalize().unwrap();
    seed_session(&state, session_id).await;

    let binding = AgentBindingService::new(state.db())
        .upsert_binding(UpsertAgentBindingRequest {
            session_id: session_id.to_string(),
            client_type: "pi".to_string(),
            launch_cwd: cwd.to_string_lossy().to_string(),
            client_session_key: "missing-session-key".to_string(),
            metadata: json!({}),
        })
        .await
        .unwrap();
    sqlx::query("UPDATE agent_bindings SET discovered = TRUE WHERE id = ?")
        .bind(&binding.id)
        .execute(&state.db())
        .await
        .unwrap();
    seed_session_exited(&state, session_id).await;

    let (status, body) = get_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/timeline"),
    )
    .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"]["code"], "source_unavailable");

    unsafe { std::env::remove_var("PI_AGENT_DIR") };
}

#[tokio::test]
async fn timeline_external_api_returns_not_ready_for_missing_discovered_source_in_active_session() {
    let _guard = PI_AGENT_DIR_ENV_LOCK.lock().await;
    let temp = tempdir().unwrap();
    let agent_dir = temp.path().join("agent");
    unsafe { std::env::set_var("PI_AGENT_DIR", &agent_dir) };

    let state = test_state().await;
    let session_id = "sess_raw_active_missing_source";
    let cwd = temp.path().join("workspace");
    fs::create_dir_all(&cwd).unwrap();
    let cwd = cwd.canonicalize().unwrap();
    seed_session(&state, session_id).await;

    let binding = AgentBindingService::new(state.db())
        .upsert_binding(UpsertAgentBindingRequest {
            session_id: session_id.to_string(),
            client_type: "pi".to_string(),
            launch_cwd: cwd.to_string_lossy().to_string(),
            client_session_key: "missing-active-session-key".to_string(),
            metadata: json!({}),
        })
        .await
        .unwrap();
    sqlx::query("UPDATE agent_bindings SET discovered = TRUE WHERE id = ?")
        .bind(&binding.id)
        .execute(&state.db())
        .await
        .unwrap();

    let (status, body) = get_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/timeline"),
    )
    .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"]["code"], "not_ready");

    unsafe { std::env::remove_var("PI_AGENT_DIR") };
}

#[tokio::test]
async fn timeline_external_api_does_not_mark_binding_discovered_until_timeline_parse_succeeds() {
    let _guard = PI_AGENT_DIR_ENV_LOCK.lock().await;
    let temp = tempdir().unwrap();
    let agent_dir = temp.path().join("agent");
    unsafe { std::env::set_var("PI_AGENT_DIR", &agent_dir) };

    let state = test_state().await;
    let session_id = "sess_raw_not_discovered_until_parse";
    let session_key = "sess_parse_must_succeed";
    let cwd = temp.path().join("workspace");
    fs::create_dir_all(&cwd).unwrap();
    let cwd = cwd.canonicalize().unwrap();
    seed_session(&state, session_id).await;

    let session_dir = pi_session_dir(&agent_dir, &cwd);
    fs::create_dir_all(&session_dir).unwrap();
    let source_path = session_dir.join(format!("2026-06-09T00-00-00-000Z_{session_key}.jsonl"));
    fs::create_dir_all(&source_path).unwrap();

    let binding = AgentBindingService::new(state.db())
        .upsert_binding(UpsertAgentBindingRequest {
            session_id: session_id.to_string(),
            client_type: "pi".to_string(),
            launch_cwd: cwd.to_string_lossy().to_string(),
            client_session_key: session_key.to_string(),
            metadata: json!({}),
        })
        .await
        .unwrap();

    let (status, body) = get_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/timeline"),
    )
    .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"]["code"], "not_ready");
    let discovered: bool = sqlx::query_scalar("SELECT discovered FROM agent_bindings WHERE id = ?")
        .bind(&binding.id)
        .fetch_one(&state.db())
        .await
        .unwrap();
    assert!(!discovered);

    fs::remove_dir(&source_path).unwrap();
    let (status, body) = get_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/timeline"),
    )
    .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"]["code"], "not_ready");

    unsafe { std::env::remove_var("PI_AGENT_DIR") };
}

#[tokio::test]
async fn timeline_external_api_marks_binding_discovered_after_first_successful_parse() {
    let _guard = PI_AGENT_DIR_ENV_LOCK.lock().await;
    let temp = tempdir().unwrap();
    let agent_dir = temp.path().join("agent");
    unsafe { std::env::set_var("PI_AGENT_DIR", &agent_dir) };

    let state = test_state().await;
    let session_id = "sess_raw_marks_discovered";
    let session_key = "sess_marks_discovered_key";
    let cwd = temp.path().join("workspace");
    fs::create_dir_all(&cwd).unwrap();
    let cwd = cwd.canonicalize().unwrap();
    seed_session(&state, session_id).await;

    let session_dir = pi_session_dir(&agent_dir, &cwd);
    fs::create_dir_all(&session_dir).unwrap();
    fs::write(
        session_dir.join(format!("2026-06-09T00-00-00-000Z_{session_key}.jsonl")),
        "{\"type\":\"message\",\"id\":\"u1\",\"message\":{\"role\":\"user\",\"content\":\"hello\"}}\n",
    )
    .unwrap();

    let binding = AgentBindingService::new(state.db())
        .upsert_binding(UpsertAgentBindingRequest {
            session_id: session_id.to_string(),
            client_type: "pi".to_string(),
            launch_cwd: cwd.to_string_lossy().to_string(),
            client_session_key: session_key.to_string(),
            metadata: json!({}),
        })
        .await
        .unwrap();

    let (status, _body) = get_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/timeline"),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let discovered: bool = sqlx::query_scalar("SELECT discovered FROM agent_bindings WHERE id = ?")
        .bind(&binding.id)
        .fetch_one(&state.db())
        .await
        .unwrap();
    assert!(discovered);

    unsafe { std::env::remove_var("PI_AGENT_DIR") };
}

#[tokio::test]
async fn timeline_external_api_returns_not_ready_without_binding() {
    let state = test_state().await;
    let session_id = "sess_raw_not_ready";
    seed_session(&state, session_id).await;

    let (status, body) = get_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/timeline"),
    )
    .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"]["code"], "not_ready");
}

fn urlencoding_for_test(value: &str) -> String {
    value.replace(':', "%3A")
}
