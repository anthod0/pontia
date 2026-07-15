use crate::test_app::TestApp;
use std::{
    fs,
    io::Write,
    sync::{Arc, Mutex as StdMutex},
};

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use pontia_application::{
    AgentBindingService, AppState, EventIngestService, UpsertAgentBindingRequest,
};
use pontia_core::domain::{
    EventSource, EventType, ProjectionState, ReportedEvent, TimelineBoundary,
};
use pontia_http as http;
use serde_json::{Value, json};
use tempfile::tempdir;
use tokio::sync::Mutex;
use tower::ServiceExt;
use tracing::instrument::WithSubscriber;
use tracing_subscriber::fmt::MakeWriter;

const TOKEN: &str = "test-token";
static PI_AGENT_DIR_ENV_LOCK: Mutex<()> = Mutex::const_new(());

#[derive(Clone, Default)]
struct CapturedLogWriter(Arc<StdMutex<Vec<u8>>>);

impl CapturedLogWriter {
    fn text(&self) -> String {
        String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
    }
}

impl Write for CapturedLogWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'writer> MakeWriter<'writer> for CapturedLogWriter {
    type Writer = Self;

    fn make_writer(&'writer self) -> Self::Writer {
        self.clone()
    }
}

async fn test_state() -> AppState {
    TestApp::builder()
        .database_name("raw-transcript-api.db")
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
    (status, serde_json::from_slice(&body).expect("json body"))
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

async fn seed_session_for_client(state: &AppState, session_id: &str, client_type: &str) {
    let service = EventIngestService::new(state.db());
    service
        .ingest_event(ReportedEvent::new(
            format!("evt_{session_id}_created"),
            session_id.to_string(),
            None,
            EventSource::AgentAdapter,
            client_type.to_string(),
            EventType::SessionCreated,
            json!({}),
        ))
        .await
        .unwrap();
}

async fn seed_session(state: &AppState, session_id: &str) {
    seed_session_for_client(state, session_id, "pi").await;
}

async fn seed_session_exited(state: &AppState, session_id: &str) {
    EventIngestService::new(state.db())
        .ingest_event(ReportedEvent::new(
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
async fn hook_lifecycle_events_capture_project_and_replay_pi_v2_boundaries() {
    let _guard = PI_AGENT_DIR_ENV_LOCK.lock().await;
    let temp = tempdir().unwrap();
    let agent_dir = temp.path().join("agent");
    unsafe { std::env::set_var("PI_AGENT_DIR", &agent_dir) };

    let state = test_state().await;
    let session_id = "sess_pi_boundaries";
    let turn_id = "turn_pi_boundaries";
    let session_key = "pi-boundary-session";
    let cwd = temp.path().join("workspace");
    fs::create_dir_all(&cwd).unwrap();
    let cwd = cwd.canonicalize().unwrap();
    seed_session(&state, session_id).await;

    let session_dir = pi_session_dir(&agent_dir, &cwd);
    fs::create_dir_all(&session_dir).unwrap();
    let transcript = session_dir.join(format!("2026-07-15T00-00-00-000Z_{session_key}.jsonl"));
    fs::write(
        &transcript,
        b"{\"type\":\"message\",\"id\":\"previous_leaf\",\"parentId\":null}\n",
    )
    .unwrap();
    let head_offset = fs::metadata(&transcript).unwrap().len();

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

    let started = json!({
        "event_id": "evt_pi_boundary_started",
        "session_id": session_id,
        "turn_id": turn_id,
        "source": "agent_adapter",
        "client_type": "pi",
        "type": "turn.started",
        "time": "2026-07-15T00:00:01Z",
        "seq": null,
        "payload": {
            "runtime_instance_id": "rtinst_pi_boundary",
            "timeline_anchor": { "previous_leaf_id": "previous_leaf" }
        }
    });
    assert_eq!(
        post_internal_event(state.clone(), started).await.0,
        StatusCode::OK
    );

    fs::OpenOptions::new()
        .append(true)
        .open(&transcript)
        .unwrap()
        .write_all(
            concat!(
                "{\"type\":\"message\",\"id\":\"user_leaf\",\"parentId\":\"previous_leaf\"}\n",
                "{\"type\":\"message\",\"id\":\"terminal_leaf\",\"parentId\":\"user_leaf\"}\n"
            )
            .as_bytes(),
        )
        .unwrap();
    let tail_offset = fs::metadata(&transcript).unwrap().len();

    let completed = json!({
        "event_id": "evt_pi_boundary_completed",
        "session_id": session_id,
        "turn_id": turn_id,
        "source": "agent_adapter",
        "client_type": "pi",
        "type": "turn.completed",
        "time": "2026-07-15T00:00:02Z",
        "seq": null,
        "payload": {
            "runtime_instance_id": "rtinst_pi_boundary",
            "timeline_anchor": { "terminal_leaf_id": "terminal_leaf" }
        }
    });
    assert_eq!(
        post_internal_event(state.clone(), completed).await.0,
        StatusCode::OK
    );

    let expected_head = format!(
        "pi-jsonl-v2:{}:{head_offset}:after:previous_leaf",
        binding.id
    );
    let expected_tail = format!(
        "pi-jsonl-v2:{}:{tail_offset}:after:terminal_leaf",
        binding.id
    );
    let (status, body) = get_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/turns/{turn_id}"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");
    assert_eq!(body["data"]["turn"]["turn_index"], 1);
    assert_eq!(body["data"]["turn"]["head_cursor"], expected_head);
    assert_eq!(body["data"]["turn"]["tail_cursor"], expected_tail);

    let events = EventIngestService::new(state.db())
        .list_events(session_id)
        .await
        .unwrap();
    assert_eq!(
        events[1].payload["timeline_anchor"]["previous_leaf_id"],
        "previous_leaf"
    );
    assert_eq!(
        events[1].timeline_boundary,
        Some(TimelineBoundary::head(expected_head.clone()))
    );
    assert!(events[1].payload.get("timeline_boundary").is_none());
    let mut replay = ProjectionState::default();
    for event in &events {
        replay.apply(event).unwrap();
    }
    let replayed = replay.turn(turn_id).unwrap();
    assert_eq!(
        replayed.head_cursor.as_deref(),
        Some(expected_head.as_str())
    );
    assert_eq!(
        replayed.tail_cursor.as_deref(),
        Some(expected_tail.as_str())
    );
}

#[tokio::test]
async fn first_pi_turn_accepts_a_null_previous_leaf_when_that_turn_was_precreated() {
    let _guard = PI_AGENT_DIR_ENV_LOCK.lock().await;
    let temp = tempdir().unwrap();
    let agent_dir = temp.path().join("agent");
    unsafe { std::env::set_var("PI_AGENT_DIR", &agent_dir) };
    let state = test_state().await;
    let session_id = "sess_pi_first_null_boundary";
    let turn_id = "turn_pi_first_null_boundary";
    let session_key = "pi-first-null-boundary";
    let cwd = temp.path().join("workspace");
    fs::create_dir_all(&cwd).unwrap();
    let cwd = cwd.canonicalize().unwrap();
    seed_session(&state, session_id).await;

    let session_dir = pi_session_dir(&agent_dir, &cwd);
    fs::create_dir_all(&session_dir).unwrap();
    fs::write(
        session_dir.join(format!("2026-07-15T00-00-00-000Z_{session_key}.jsonl")),
        b"",
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
    EventIngestService::new(state.db())
        .ingest_event(ReportedEvent::new(
            "evt_pi_first_null_created".to_string(),
            session_id.to_string(),
            Some(turn_id.to_string()),
            EventSource::ExternalApi,
            "pi".to_string(),
            EventType::TurnCreated,
            json!({}),
        ))
        .await
        .unwrap();

    let started = json!({
        "event_id": "evt_pi_first_null_started",
        "session_id": session_id,
        "turn_id": turn_id,
        "source": "agent_adapter",
        "client_type": "pi",
        "type": "turn.started",
        "time": "2026-07-15T00:00:01Z",
        "seq": null,
        "payload": {
            "runtime_instance_id": "rtinst_pi_first_null",
            "timeline_anchor": { "previous_leaf_id": null }
        }
    });
    let (status, body) = post_internal_event(state.clone(), started).await;
    assert_eq!(status, StatusCode::OK, "{body:?}");

    let turn = EventIngestService::new(state.db())
        .get_turn(turn_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        turn.head_cursor.as_deref(),
        Some(format!("pi-jsonl-v2:{}:0:after:", binding.id).as_str())
    );
}

#[tokio::test]
async fn timeline_capture_failure_keeps_lifecycle_fact_without_cursor_or_database_warning() {
    let _guard = PI_AGENT_DIR_ENV_LOCK.lock().await;
    let temp = tempdir().unwrap();
    let agent_dir = temp.path().join("missing-agent-dir");
    unsafe { std::env::set_var("PI_AGENT_DIR", &agent_dir) };
    let state = test_state().await;
    let session_id = "sess_pi_boundary_missing";
    seed_session(&state, session_id).await;
    let binding = AgentBindingService::new(state.db())
        .upsert_binding(UpsertAgentBindingRequest {
            session_id: session_id.to_string(),
            client_type: "pi".to_string(),
            launch_cwd: temp.path().join("workspace").display().to_string(),
            client_session_key: "missing-session".to_string(),
            metadata: json!({}),
        })
        .await
        .unwrap();
    let started = json!({
        "event_id": "evt_pi_boundary_missing_started",
        "session_id": session_id,
        "turn_id": "turn_pi_boundary_missing",
        "source": "agent_adapter",
        "client_type": "pi",
        "type": "turn.started",
        "time": "2026-07-15T00:00:01Z",
        "seq": null,
        "payload": {
            "runtime_instance_id": "rtinst_pi_boundary_missing",
            "timeline_anchor": { "previous_leaf_id": null }
        }
    });

    let captured_logs = CapturedLogWriter::default();
    let subscriber = tracing_subscriber::fmt()
        .json()
        .without_time()
        .with_writer(captured_logs.clone())
        .finish();
    let (status, body) = post_internal_event(state.clone(), started)
        .with_subscriber(subscriber)
        .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");
    let turn = EventIngestService::new(state.db())
        .get_turn("turn_pi_boundary_missing")
        .await
        .unwrap()
        .unwrap();
    assert!(turn.head_cursor.is_none());
    let warning_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM ingest_warnings WHERE event_id = 'evt_pi_boundary_missing_started'",
    )
    .fetch_one(&state.db())
    .await
    .unwrap();
    assert_eq!(warning_count, 0);

    let warning = captured_logs
        .text()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .find(|entry| entry["fields"]["code"] == "timeline_boundary_capture_failed")
        .expect("structured timeline capture warning");
    assert_eq!(warning["level"], "WARN");
    assert_eq!(
        warning["fields"]["event_id"],
        "evt_pi_boundary_missing_started"
    );
    assert_eq!(warning["fields"]["session_id"], session_id);
    assert_eq!(warning["fields"]["turn_id"], "turn_pi_boundary_missing");
    assert_eq!(warning["fields"]["event_type"], "turn.started");
    assert_eq!(warning["fields"]["client_type"], "pi");
    assert_eq!(warning["fields"]["binding_id"], binding.id);
    assert_eq!(warning["fields"]["adapter_error"], "source_unavailable");
    assert!(
        !captured_logs
            .text()
            .contains(&temp.path().display().to_string())
    );
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
    assert!(body["data"]["head_cursor"].is_null());
    assert!(body["data"]["tail_cursor"].as_str().is_some());
    assert!(body["data"].get("older_cursor").is_none());
    assert!(body["data"].get("is_tail").is_none());
    assert!(body["data"].get("next_cursor").is_none());
    assert!(
        body["data"]["source_id"]
            .as_str()
            .unwrap()
            .starts_with("pi:")
    );

    let tail_cursor = body["data"]["tail_cursor"].as_str().unwrap();
    fs::OpenOptions::new()
        .append(true)
        .open(session_dir.join(format!("2026-06-09T00-00-00-000Z_{session_key}.jsonl")))
        .unwrap()
        .write_all(
            b"{\"type\":\"message\",\"id\":\"u2\",\"timestamp\":\"2026-06-09T00:00:03.000Z\",\"message\":{\"role\":\"user\",\"content\":\"new update\"}}\n",
        )
        .unwrap();
    let (updates_status, updates_body) = get_json(
        state.clone(),
        &format!(
            "/external/v1/sessions/{session_id}/timeline?after={}",
            urlencoding_for_test(tail_cursor)
        ),
    )
    .await;
    assert_eq!(updates_status, StatusCode::OK);
    assert_eq!(updates_body["data"]["items"].as_array().unwrap().len(), 1);
    assert_eq!(
        updates_body["data"]["items"][0]["item_id"],
        "pi:entry:u2:block:0"
    );
    assert!(updates_body["data"]["tail_cursor"].as_str().is_some());
    assert!(updates_body["data"].get("after_item_id").is_none());
    assert!(updates_body["data"].get("anchor_found").is_none());

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
        &format!("/external/v1/sessions/{session_id}/timeline?before=bad-cursor"),
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
async fn timeline_external_api_rejects_agent_and_runtime_binding_identity_disagreement() {
    let state = test_state().await;
    let session_id = "sess_raw_binding_disagreement";
    let cwd = tempdir().unwrap();
    let cwd = cwd.path().canonicalize().unwrap();
    seed_session(&state, session_id).await;
    AgentBindingService::new(state.db())
        .upsert_binding(UpsertAgentBindingRequest {
            session_id: session_id.to_string(),
            client_type: "pi".to_string(),
            launch_cwd: cwd.to_string_lossy().to_string(),
            client_session_key: "pi_agent_identity".to_string(),
            metadata: json!({}),
        })
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO runtime_bindings (session_id, runtime_kind, runtime_instance_id, launch_cwd, metadata) VALUES (?, 'tmux', 'rtinst_disagreement', ?, ?)",
    )
    .bind(session_id)
    .bind(cwd.to_string_lossy().to_string())
    .bind(json!({
        "client_session_key": "pi_runtime_identity",
        "runtime_instance_id": "rtinst_disagreement",
        "workspace": cwd.to_string_lossy().to_string()
    }).to_string())
    .execute(&state.db())
    .await
    .expect("Runtime binding");

    let (status, body) = get_json(
        state,
        &format!("/external/v1/sessions/{session_id}/timeline"),
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT, "{body:?}");
    assert_eq!(body["error"]["code"], "state_conflict");
    assert!(
        body["error"]["message"]
            .as_str()
            .expect("error message")
            .contains("does not match")
    );
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

#[tokio::test]
async fn timeline_external_api_uses_client_spec_to_reject_unsupported_transcripts() {
    let state = test_state().await;
    let session_id = "sess_generic_no_timeline";
    let cwd = tempdir().unwrap();
    seed_session_for_client(&state, session_id, "generic").await;

    AgentBindingService::new(state.db())
        .upsert_binding(UpsertAgentBindingRequest {
            session_id: session_id.to_string(),
            client_type: "generic".to_string(),
            launch_cwd: cwd.path().to_string_lossy().to_string(),
            client_session_key: "generic-session-key".to_string(),
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
    assert_eq!(body["error"]["code"], "capability_unavailable");
}

fn urlencoding_for_test(value: &str) -> String {
    value.replace(':', "%3A")
}
