use crate::test_app::TestApp;
use std::{
    fs,
    io::Write,
    path::PathBuf,
    sync::{Arc, Mutex as StdMutex},
};

use axum::{
    body::Body,
    http::{HeaderMap, Request, StatusCode, header},
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
    let (status, _, body) = get_response(state, uri).await;
    (status, body)
}

async fn get_response(state: AppState, uri: &str) -> (StatusCode, HeaderMap, Value) {
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
    let headers = response.headers().clone();
    let body = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    let json = serde_json::from_slice(&body).expect("json body");
    (status, headers, json)
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

struct ActivePiTimelineFixture {
    _temp: tempfile::TempDir,
    state: AppState,
    session_id: &'static str,
    transcript: PathBuf,
}

async fn active_pi_timeline_fixture(
    session_id: &'static str,
    session_key: &str,
    turn_id: &str,
    started_event_id: &str,
) -> ActivePiTimelineFixture {
    let temp = tempdir().unwrap();
    let agent_dir = temp.path().join("agent");
    unsafe { std::env::set_var("PI_AGENT_DIR", &agent_dir) };
    let state = test_state().await;
    let cwd = temp.path().join("workspace");
    fs::create_dir_all(&cwd).unwrap();
    let cwd = cwd.canonicalize().unwrap();
    seed_session(&state, session_id).await;

    let session_dir = pi_session_dir(&agent_dir, &cwd);
    fs::create_dir_all(&session_dir).unwrap();
    let transcript = session_dir.join(format!("2026-07-15T00-00-00-000Z_{session_key}.jsonl"));
    fs::write(
        &transcript,
        b"{\"type\":\"message\",\"id\":\"root\",\"parentId\":null}\n",
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
    post_pi_turn_event(
        state.clone(),
        session_id,
        turn_id,
        started_event_id,
        "turn.started",
        json!({ "previous_leaf_id": "root" }),
    )
    .await;

    ActivePiTimelineFixture {
        _temp: temp,
        state,
        session_id,
        transcript,
    }
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

#[tokio::test]
async fn turn_timeline_returns_empty_for_a_session_without_turns_or_binding() {
    let state = test_state().await;
    let session_id = "sess_empty_turn_timeline";
    seed_session(&state, session_id).await;

    let (status, body) = get_json(
        state,
        &format!("/external/v1/sessions/{session_id}/turns/timeline?direction=backward"),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body:?}");
    assert_eq!(
        body["data"],
        json!({
            "session_id": session_id,
            "direction": "backward",
            "items": [],
            "next_turn_id": null,
        })
    );
}

#[tokio::test]
async fn turn_timeline_validates_queries_anchors_and_complete_ranges() {
    let state = test_state().await;
    let session_id = "sess_turn_timeline_errors";
    seed_session(&state, session_id).await;

    for query in [
        "",
        "?direction=sideways",
        "?direction=forward&limit=0",
        "?direction=backward&limit=101",
        "?direction=forward&limit=abc",
    ] {
        let (status, body) = get_json(
            state.clone(),
            &format!("/external/v1/sessions/{session_id}/turns/timeline{query}"),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body:?}");
        assert_eq!(body["error"]["code"], "invalid_timeline_query");
    }

    let (status, body) = get_json(
        state.clone(),
        "/external/v1/sessions/missing/turns/timeline?direction=forward",
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body:?}");
    assert_eq!(body["error"]["code"], "session_not_found");

    EventIngestService::new(state.db())
        .ingest_event(ReportedEvent::new(
            "evt_unsealed_turn".to_string(),
            session_id.to_string(),
            Some("turn_unsealed".to_string()),
            EventSource::ExternalApi,
            "pi".to_string(),
            EventType::TurnCreated,
            json!({}),
        ))
        .await
        .unwrap();
    let (status, body) = get_json(
        state.clone(),
        &format!(
            "/external/v1/sessions/{session_id}/turns/timeline?direction=forward&turn_id=missing"
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body:?}");
    assert_eq!(body["error"]["code"], "turn_not_found");

    let other_session_id = "sess_turn_timeline_other";
    seed_session(&state, other_session_id).await;
    EventIngestService::new(state.db())
        .ingest_event(ReportedEvent::new(
            "evt_other_session_turn".to_string(),
            other_session_id.to_string(),
            Some("turn_other_session".to_string()),
            EventSource::ExternalApi,
            "pi".to_string(),
            EventType::TurnCreated,
            json!({}),
        ))
        .await
        .unwrap();
    let (status, body) = get_json(
        state.clone(),
        &format!(
            "/external/v1/sessions/{session_id}/turns/timeline?direction=forward&turn_id=turn_other_session"
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body:?}");
    assert_eq!(body["error"]["code"], "turn_not_found");

    let (status, body) = get_json(
        state,
        &format!("/external/v1/sessions/{session_id}/turns/timeline?direction=forward"),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body:?}");
    assert_eq!(body["error"]["code"], "turn_timeline_unavailable");
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("turn_unsealed")
    );
}

#[tokio::test]
async fn turn_timeline_only_allows_the_session_current_globally_newest_open_turn() {
    let state = test_state().await;
    let session_id = "sess_open_turn_qualification";
    seed_session(&state, session_id).await;
    sqlx::query(
        "INSERT INTO turns (turn_id, session_id, turn_index, head_cursor, state) VALUES ('turn_open', ?, 1, 'head', 'running')",
    )
    .bind(session_id)
    .execute(&state.db())
    .await
    .unwrap();

    let (status, body) = get_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/turns/timeline?direction=forward"),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body:?}");
    assert_eq!(body["error"]["code"], "turn_timeline_unavailable");

    sqlx::query("UPDATE sessions SET current_turn_id = 'turn_open' WHERE session_id = ?")
        .bind(session_id)
        .execute(&state.db())
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO turns (turn_id, session_id, turn_index, head_cursor, tail_cursor, state) VALUES ('turn_newer', ?, 2, 'head', 'tail', 'completed')",
    )
    .bind(session_id)
    .execute(&state.db())
    .await
    .unwrap();

    let (status, body) = get_json(
        state,
        &format!("/external/v1/sessions/{session_id}/turns/timeline?direction=forward&limit=1"),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body:?}");
    assert_eq!(body["error"]["code"], "turn_timeline_unavailable");
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("turn_open")
    );
}

#[tokio::test]
async fn turn_timeline_maps_capability_invalid_cursor_and_source_errors() {
    let state = test_state().await;
    let generic_session = "sess_turn_timeline_generic";
    seed_session_for_client(&state, generic_session, "generic").await;
    AgentBindingService::new(state.db())
        .upsert_binding(UpsertAgentBindingRequest {
            session_id: generic_session.to_string(),
            client_type: "generic".to_string(),
            launch_cwd: "/unused".to_string(),
            client_session_key: "generic-timeline".to_string(),
            metadata: json!({}),
        })
        .await
        .unwrap();
    insert_sealed_turn(&state, generic_session, "turn_generic", "head", "tail").await;
    let (status, body) = get_json(
        state.clone(),
        &format!("/external/v1/sessions/{generic_session}/turns/timeline?direction=forward"),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body:?}");
    assert_eq!(body["error"]["code"], "timeline_capability_unavailable");

    let _guard = PI_AGENT_DIR_ENV_LOCK.lock().await;
    let temp = tempdir().unwrap();
    let agent_dir = temp.path().join("agent");
    unsafe { std::env::set_var("PI_AGENT_DIR", &agent_dir) };
    let pi_session = "sess_turn_timeline_invalid";
    seed_session(&state, pi_session).await;
    let cwd = temp.path().join("workspace");
    fs::create_dir_all(&cwd).unwrap();
    let cwd = cwd.canonicalize().unwrap();
    let session_dir = pi_session_dir(&agent_dir, &cwd);
    fs::create_dir_all(&session_dir).unwrap();
    let source_path = session_dir.join("2026-07-15T00-00-00-000Z_invalid-cursor.jsonl");
    fs::write(&source_path, b"{\"id\":\"entry\",\"parentId\":null}\n").unwrap();
    let binding = AgentBindingService::new(state.db())
        .upsert_binding(UpsertAgentBindingRequest {
            session_id: pi_session.to_string(),
            client_type: "pi".to_string(),
            launch_cwd: cwd.to_string_lossy().to_string(),
            client_session_key: "invalid-cursor".to_string(),
            metadata: json!({}),
        })
        .await
        .unwrap();
    insert_sealed_turn(
        &state,
        pi_session,
        "turn_invalid_cursor",
        &format!("pi-jsonl-v1:{}:0:0", binding.id),
        &format!("pi-jsonl-v2:{}:39:after:entry", binding.id),
    )
    .await;
    let (status, body) = get_json(
        state.clone(),
        &format!("/external/v1/sessions/{pi_session}/turns/timeline?direction=forward"),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body:?}");
    assert_eq!(body["error"]["code"], "turn_timeline_invalid");
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("turn_invalid_cursor")
    );

    let malformed = b"{not json}\n";
    fs::write(&source_path, malformed).unwrap();
    update_turn_cursors(
        &state,
        "turn_invalid_cursor",
        &format!("pi-jsonl-v2:{}:0:after:", binding.id),
        &format!(
            "pi-jsonl-v2:{}:{}:after:terminal",
            binding.id,
            malformed.len()
        ),
    )
    .await;
    let (status, body) = get_json(
        state.clone(),
        &format!("/external/v1/sessions/{pi_session}/turns/timeline?direction=forward"),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body:?}");
    assert_eq!(body["error"]["code"], "turn_timeline_invalid");

    let broken = b"{\"id\":\"terminal\",\"parentId\":\"missing\"}\n";
    fs::write(&source_path, broken).unwrap();
    update_turn_cursors(
        &state,
        "turn_invalid_cursor",
        &format!("pi-jsonl-v2:{}:0:after:root", binding.id),
        &format!("pi-jsonl-v2:{}:{}:after:terminal", binding.id, broken.len()),
    )
    .await;
    let (status, body) = get_json(
        state.clone(),
        &format!("/external/v1/sessions/{pi_session}/turns/timeline?direction=forward"),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body:?}");
    assert_eq!(body["error"]["code"], "turn_timeline_invalid");

    fs::remove_dir_all(&session_dir).unwrap();
    let (status, body) = get_json(
        state,
        &format!("/external/v1/sessions/{pi_session}/turns/timeline?direction=forward"),
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body:?}");
    assert_eq!(body["error"]["code"], "timeline_source_unavailable");
    assert!(
        !body
            .to_string()
            .contains(&temp.path().display().to_string())
    );
}

async fn update_turn_cursors(
    state: &AppState,
    turn_id: &str,
    head_cursor: &str,
    tail_cursor: &str,
) {
    sqlx::query("UPDATE turns SET head_cursor = ?, tail_cursor = ? WHERE turn_id = ?")
        .bind(head_cursor)
        .bind(tail_cursor)
        .bind(turn_id)
        .execute(&state.db())
        .await
        .unwrap();
}

async fn insert_sealed_turn(
    state: &AppState,
    session_id: &str,
    turn_id: &str,
    head_cursor: &str,
    tail_cursor: &str,
) {
    sqlx::query(
        "INSERT INTO turns (turn_id, session_id, turn_index, head_cursor, tail_cursor, state) VALUES (?, ?, 1, ?, ?, 'completed')",
    )
    .bind(turn_id)
    .bind(session_id)
    .bind(head_cursor)
    .bind(tail_cursor)
    .execute(&state.db())
    .await
    .unwrap();
}

#[tokio::test]
async fn turn_timeline_reads_sealed_pi_ranges_and_pages_by_turn_index() {
    let _guard = PI_AGENT_DIR_ENV_LOCK.lock().await;
    let temp = tempdir().unwrap();
    let agent_dir = temp.path().join("agent");
    unsafe { std::env::set_var("PI_AGENT_DIR", &agent_dir) };
    let state = test_state().await;
    let session_id = "sess_projected_timeline";
    let session_key = "projected-timeline";
    let cwd = temp.path().join("workspace");
    fs::create_dir_all(&cwd).unwrap();
    let cwd = cwd.canonicalize().unwrap();
    seed_session(&state, session_id).await;

    let session_dir = pi_session_dir(&agent_dir, &cwd);
    fs::create_dir_all(&session_dir).unwrap();
    let transcript = session_dir.join(format!("2026-07-15T00-00-00-000Z_{session_key}.jsonl"));
    fs::write(
        &transcript,
        b"{\"type\":\"message\",\"id\":\"root\",\"parentId\":null,\"message\":{\"role\":\"user\",\"content\":\"before\"}}\n",
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

    post_pi_turn_event(
        state.clone(),
        session_id,
        "turn_one",
        "evt_turn_one_started",
        "turn.started",
        json!({ "previous_leaf_id": "root" }),
    )
    .await;
    fs::OpenOptions::new()
        .append(true)
        .open(&transcript)
        .unwrap()
        .write_all(
            concat!(
                "{\"type\":\"message\",\"id\":\"u1\",\"parentId\":\"root\",\"timestamp\":\"2026-07-15T00:00:01Z\",\"message\":{\"role\":\"user\",\"content\":\"question one\"}}\n",
                "{\"type\":\"message\",\"id\":\"a1\",\"parentId\":\"u1\",\"timestamp\":\"2026-07-15T00:00:02Z\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"answer one\"},{\"type\":\"toolCall\",\"name\":\"read\",\"arguments\":{\"path\":\"README.md\"}}]}}\n"
            )
            .as_bytes(),
        )
        .unwrap();
    post_pi_turn_event(
        state.clone(),
        session_id,
        "turn_one",
        "evt_turn_one_completed",
        "turn.completed",
        json!({ "terminal_leaf_id": "a1" }),
    )
    .await;

    post_pi_turn_event(
        state.clone(),
        session_id,
        "turn_two",
        "evt_turn_two_started",
        "turn.started",
        json!({ "previous_leaf_id": "a1" }),
    )
    .await;
    fs::OpenOptions::new()
        .append(true)
        .open(&transcript)
        .unwrap()
        .write_all(
            concat!(
                "{\"type\":\"message\",\"id\":\"u2\",\"parentId\":\"a1\",\"timestamp\":\"2026-07-15T00:00:03Z\",\"message\":{\"role\":\"user\",\"content\":\"question two\"}}\n",
                "{\"type\":\"message\",\"id\":\"a2\",\"parentId\":\"u2\",\"timestamp\":\"2026-07-15T00:00:04Z\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"answer two\"}]}}\n"
            )
            .as_bytes(),
        )
        .unwrap();
    post_pi_turn_event(
        state.clone(),
        session_id,
        "turn_two",
        "evt_turn_two_completed",
        "turn.completed",
        json!({ "terminal_leaf_id": "a2" }),
    )
    .await;

    let (status, recent) = get_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/turns/timeline?direction=backward&limit=1"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{recent:?}");
    assert_eq!(recent["data"]["next_turn_id"], "turn_one");
    assert_eq!(recent["data"]["items"].as_array().unwrap().len(), 2);
    assert!(
        recent["data"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["turn_id"] == "turn_two")
    );

    let (status, older) = get_json(
        state.clone(),
        &format!(
            "/external/v1/sessions/{session_id}/turns/timeline?direction=backward&turn_id=turn_one&limit=1"
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{older:?}");
    assert!(older["data"]["next_turn_id"].is_null());
    assert_eq!(older["data"]["items"][0]["content_preview"], "question one");
    assert_eq!(
        older["data"]["items"][2]["managed_tool_use"]["tool_name"],
        "read"
    );

    let (status, all) = get_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/turns/timeline?direction=forward"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{all:?}");
    let turn_ids = all["data"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["turn_id"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        turn_ids,
        vec!["turn_one", "turn_one", "turn_one", "turn_two", "turn_two"]
    );
    let content_ref = all["data"]["items"][0]["content_ref"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(content_ref.starts_with("pi-jsonl-ref-v1:"));
    let (status, detail) = get_json(
        state,
        &format!("/external/v1/sessions/{session_id}/timeline/detail?ref={content_ref}"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{detail:?}");
    assert!(
        detail["data"]["text"]
            .as_str()
            .unwrap()
            .contains("question one")
    );
}

#[tokio::test]
async fn turn_timeline_reads_growing_active_output_without_persisting_temporary_boundaries() {
    let _guard = PI_AGENT_DIR_ENV_LOCK.lock().await;
    let fixture = active_pi_timeline_fixture(
        "sess_active_turn_empty",
        "active-turn-empty",
        "turn_active",
        "evt_active_turn_started",
    )
    .await;
    let state = fixture.state.clone();
    let session_id = fixture.session_id;
    let transcript = &fixture.transcript;

    let (status, body) = get_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/turns/timeline?direction=backward"),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body:?}");
    assert_eq!(body["data"]["items"], json!([]));
    assert!(body["data"]["next_turn_id"].is_null());

    fs::OpenOptions::new()
        .append(true)
        .open(&transcript)
        .unwrap()
        .write_all(
            concat!(
                "{\"type\":\"message\",\"id\":\"user\",\"parentId\":\"root\",\"message\":{\"role\":\"user\",\"content\":\"question\"}}\n",
                "{\"type\":\"message\",\"id\":\"answer\",\"parentId\":\"user\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"partial answer\"}]}}\n"
            )
            .as_bytes(),
        )
        .unwrap();
    let (status, growing) = get_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/turns/timeline?direction=forward"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{growing:?}");
    assert_eq!(growing["data"]["items"].as_array().unwrap().len(), 2);
    assert_eq!(
        growing["data"]["items"][1]["content_preview"],
        "partial answer"
    );

    let active_turn = EventIngestService::new(state.db())
        .get_turn("turn_active")
        .await
        .unwrap()
        .unwrap();
    assert!(active_turn.head_cursor.is_some());
    assert!(active_turn.tail_cursor.is_none());

    fs::OpenOptions::new()
        .append(true)
        .open(&transcript)
        .unwrap()
        .write_all(
            b"{\"type\":\"message\",\"id\":\"final\",\"parentId\":\"answer\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"final answer\"}]}}\n",
        )
        .unwrap();
    let (status, grown) = get_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/turns/timeline?direction=forward"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{grown:?}");
    assert_eq!(grown["data"]["items"].as_array().unwrap().len(), 3);
    assert_eq!(grown["data"]["items"][2]["content_preview"], "final answer");
    assert!(
        EventIngestService::new(state.db())
            .get_turn("turn_active")
            .await
            .unwrap()
            .unwrap()
            .tail_cursor
            .is_none()
    );

    post_pi_turn_event(
        state.clone(),
        session_id,
        "turn_active",
        "evt_active_turn_completed",
        "turn.completed",
        json!({ "terminal_leaf_id": "final" }),
    )
    .await;
    let (status, sealed) = get_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/turns/timeline?direction=forward"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{sealed:?}");
    assert_eq!(sealed["data"]["items"], grown["data"]["items"]);
    assert!(
        EventIngestService::new(state.db())
            .get_turn("turn_active")
            .await
            .unwrap()
            .unwrap()
            .tail_cursor
            .is_some()
    );
}

#[tokio::test]
async fn turn_timeline_rejects_unassignable_active_pi_entries() {
    let _guard = PI_AGENT_DIR_ENV_LOCK.lock().await;
    let fixture = active_pi_timeline_fixture(
        "sess_active_turn_unassignable",
        "active-turn-unassignable",
        "turn_active_invalid",
        "evt_active_turn_invalid_started",
    )
    .await;
    let state = fixture.state.clone();
    let session_id = fixture.session_id;
    let transcript = &fixture.transcript;
    fs::OpenOptions::new()
        .append(true)
        .open(&transcript)
        .unwrap()
        .write_all(
            concat!(
                "{\"type\":\"message\",\"parentId\":\"root\",\"message\":{\"role\":\"user\",\"content\":\"missing id\"}}\n",
                "{\"type\":\"message\",\"id\":\"answer\",\"parentId\":\"root\",\"message\":{\"role\":\"assistant\",\"content\":\"answer\"}}\n"
            )
            .as_bytes(),
        )
        .unwrap();

    let (status, body) = get_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/turns/timeline?direction=forward"),
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT, "{body:?}");
    assert_eq!(body["error"]["code"], "turn_timeline_invalid");
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("turn_active_invalid")
    );

    let root = b"{\"type\":\"message\",\"id\":\"root\",\"parentId\":null}\n";
    for invalid_suffix in [
        concat!(
            "{\"type\":\"message\",\"id\":\"branch_one\",\"parentId\":\"root\"}\n",
            "{\"type\":\"message\",\"id\":\"branch_two\",\"parentId\":\"root\"}\n"
        )
        .as_bytes(),
        b"{not json}\n".as_slice(),
        b"{\"type\":\"message\",\"id\":\"partial\",\"parentId\":\"root\"}".as_slice(),
    ] {
        fs::write(&transcript, [root.as_slice(), invalid_suffix].concat()).unwrap();
        let (status, body) = get_json(
            state.clone(),
            &format!("/external/v1/sessions/{session_id}/turns/timeline?direction=forward"),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{body:?}");
        assert_eq!(body["error"]["code"], "turn_timeline_invalid");
    }
}

async fn post_pi_turn_event(
    state: AppState,
    session_id: &str,
    turn_id: &str,
    event_id: &str,
    event_type: &str,
    timeline_anchor: Value,
) {
    let (status, body) = post_internal_event(
        state,
        json!({
            "event_id": event_id,
            "session_id": session_id,
            "turn_id": turn_id,
            "source": "agent_adapter",
            "client_type": "pi",
            "type": event_type,
            "time": "2026-07-15T00:00:00Z",
            "seq": null,
            "payload": {
                "runtime_instance_id": "rtinst_projected_timeline",
                "timeline_anchor": timeline_anchor,
            }
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");
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

    let (status, response_headers, body) = get_response(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/timeline?limit=1"),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(response_headers["Deprecation"], "@1784073600");
    assert_eq!(
        response_headers[header::LINK],
        format!("</external/v1/sessions/{session_id}/turns/timeline>; rel=\"successor-version\"")
    );
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
