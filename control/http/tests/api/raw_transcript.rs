use crate::test_app::TestApp;
use std::{
    fs,
    io::Write,
    path::PathBuf,
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

#[tokio::test]
async fn legacy_session_timeline_endpoints_are_removed() {
    let state = test_state().await;
    for uri in [
        "/external/v1/sessions/session-1/timeline",
        "/external/v1/sessions/session-1/timeline/detail?ref=old-ref",
    ] {
        let response = http::router(state.clone())
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

        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{uri}");
    }
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
    _started_event_id: &str,
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
    precreate_turn_if_missing(&state, session_id, turn_id).await;
    let (status, body) = post_internal_event(
        state.clone(),
        json!({
            "session_id": session_id,
            "turn_id": turn_id,
            "type": "turn.started",
            "data": {
                "runtime_instance_id": "rtinst_projected_timeline",
                "timeline_anchor": { "previous_leaf_id": "root" },
                "topology_context": { "entries": [] },
            }
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");

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
        .ingest_reported_event(ReportedEvent::new(
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

async fn precreate_turn_if_missing(state: &AppState, session_id: &str, turn_id: &str) {
    let service = EventIngestService::new(state.db());
    if service.get_turn(turn_id).await.unwrap().is_some() {
        return;
    }
    service
        .ingest_reported_event(ReportedEvent::new(
            format!("evt_precreate_{turn_id}"),
            session_id.to_string(),
            Some(turn_id.to_string()),
            EventSource::ExternalApi,
            "pi".to_string(),
            EventType::TurnCreated,
            json!({}),
        ))
        .await
        .unwrap();
}

#[tokio::test]
async fn turn_timeline_returns_empty_for_a_session_without_turns_or_binding() {
    let state = test_state().await;
    let session_id = "sess_empty_turn_timeline";
    seed_session(&state, session_id).await;

    let (status, body) = get_json(
        state.clone(),
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

    let (status, history) = get_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/turns/tree/history"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{history:?}");
    assert_eq!(
        history["data"],
        json!({
            "session_id": session_id,
            "groups": [],
            "next_from_turn_id": null,
        })
    );

    let (status, updates) = get_json(
        state,
        &format!("/external/v1/sessions/{session_id}/turns/tree/updates"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{updates:?}");
    assert_eq!(
        updates["data"],
        json!({
            "session_id": session_id,
            "current_turn_id": null,
            "retain_through_turn_id": null,
            "groups": [],
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
        .ingest_reported_event(ReportedEvent::new(
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
        .ingest_reported_event(ReportedEvent::new(
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
async fn turn_timeline_only_allows_the_globally_newest_active_turn() {
    let state = test_state().await;
    let session_id = "sess_open_turn_qualification";
    seed_session(&state, session_id).await;
    sqlx::query(
        "INSERT INTO turns (turn_id, session_id, head_cursor, state) VALUES ('turn_01900000-0000-7000-8000-000000000001', ?, 'head', 'running')",
    )
    .bind(session_id)
    .execute(&state.db())
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO turns (turn_id, session_id, head_cursor, tail_cursor, state) VALUES ('turn_01900000-0000-7000-8000-000000000002', ?, 'head', 'tail', 'completed')",
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
            .contains("turn_01900000-0000-7000-8000-000000000001")
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
        "INSERT INTO turns (turn_id, session_id, head_cursor, tail_cursor, state) VALUES (?, ?, ?, ?, 'completed')",
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
async fn first_turn_timeline_survives_pi_creating_its_jsonl_after_turn_start() {
    let _guard = PI_AGENT_DIR_ENV_LOCK.lock().await;
    let temp = tempdir().unwrap();
    let agent_dir = temp.path().join("agent");
    unsafe { std::env::set_var("PI_AGENT_DIR", &agent_dir) };
    let state = test_state().await;
    let session_id = "sess_delayed_first_timeline";
    let session_key = "delayed-first-timeline";
    let cwd = temp.path().join("workspace");
    fs::create_dir_all(&cwd).unwrap();
    let cwd = cwd.canonicalize().unwrap();
    seed_session(&state, session_id).await;

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

    precreate_turn_if_missing(&state, session_id, "turn_delayed_first").await;
    let (status, body) = post_internal_event(
        state.clone(),
        json!({
            "session_id": session_id,
            "turn_id": "turn_delayed_first",
            "type": "turn.started",
            "data": {
                "runtime_instance_id": "rtinst_projected_timeline",
                "timeline_anchor": { "previous_leaf_id": "previous" },
                "topology_context": { "entries": [
                    {"id": "previous", "kind": "model_change"}
                ] }
            }
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");
    let started_turn = EventIngestService::new(state.db())
        .get_turn("turn_delayed_first")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        started_turn.head_cursor.as_deref(),
        Some(format!("pi-jsonl-v2:{}:0:after:previous", binding.id).as_str())
    );
    assert_eq!(
        started_turn.topology,
        pontia_core::domain::TurnTopology::Root
    );

    let (pending_status, pending_body) = get_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/turns/timeline?direction=backward"),
    )
    .await;
    assert_eq!(pending_status, StatusCode::OK, "{pending_body:?}");
    assert_eq!(pending_body["data"]["items"], json!([]));
    assert!(pending_body["data"]["next_turn_id"].is_null());

    let session_dir = pi_session_dir(&agent_dir, &cwd);
    fs::create_dir_all(&session_dir).unwrap();
    fs::write(
        session_dir.join(format!("2026-07-15T00-00-00-000Z_{session_key}.jsonl")),
        concat!(
            "{\"type\":\"session\",\"id\":\"native-session\"}\n",
            "{\"type\":\"model_change\",\"id\":\"previous\",\"parentId\":null}\n",
            "{\"type\":\"message\",\"id\":\"user\",\"parentId\":\"previous\",\"message\":{\"role\":\"user\",\"content\":\"first question\"}}\n",
            "{\"type\":\"message\",\"id\":\"answer\",\"parentId\":\"user\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"first answer\"}]}}\n",
        ),
    )
    .unwrap();
    let (active_status, active_body) = get_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/turns/timeline?direction=backward"),
    )
    .await;
    assert_eq!(active_status, StatusCode::OK, "{active_body:?}");
    assert_eq!(active_body["data"]["items"].as_array().unwrap().len(), 2);
    let discovered: bool = sqlx::query_scalar("SELECT discovered FROM agent_bindings WHERE id = ?")
        .bind(&binding.id)
        .fetch_one(&state.db())
        .await
        .unwrap();
    assert!(discovered);

    post_pi_turn_event(
        state.clone(),
        session_id,
        "turn_delayed_first",
        "evt_delayed_first_completed",
        "turn.completed",
        json!({ "terminal_leaf_id": "answer" }),
    )
    .await;

    let (status, body) = get_json(
        state,
        &format!("/external/v1/sessions/{session_id}/turns/timeline?direction=backward"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");
    assert_eq!(
        body["data"]["items"][0]["content_preview"],
        "first question"
    );
    assert_eq!(body["data"]["items"][1]["content_preview"], "first answer");
}

#[tokio::test]
async fn delayed_terminal_fact_seals_timeline_after_runtime_binding_changes() {
    let _guard = PI_AGENT_DIR_ENV_LOCK.lock().await;
    let temp = tempdir().unwrap();
    let agent_dir = temp.path().join("agent");
    unsafe { std::env::set_var("PI_AGENT_DIR", &agent_dir) };
    let state = test_state().await;
    let session_id = "sess_delayed_terminal";
    let session_key = "delayed-terminal";
    let turn_id = "turn_delayed_terminal";
    let cwd = temp.path().join("workspace");
    fs::create_dir_all(&cwd).unwrap();
    let cwd = cwd.canonicalize().unwrap();
    seed_session(&state, session_id).await;

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
    sqlx::query(
        "INSERT INTO runtime_bindings (session_id, runtime_kind, runtime_instance_id, metadata) VALUES (?, 'pi_tui', 'rtinst_a', '{}')",
    )
    .bind(session_id)
    .execute(&state.db())
    .await
    .unwrap();

    let session_dir = pi_session_dir(&agent_dir, &cwd);
    fs::create_dir_all(&session_dir).unwrap();
    let transcript = session_dir.join(format!("2026-07-15T00-00-00-000Z_{session_key}.jsonl"));
    fs::write(
        &transcript,
        b"{\"type\":\"model_change\",\"id\":\"previous\",\"parentId\":null}\n",
    )
    .unwrap();

    precreate_turn_if_missing(&state, session_id, turn_id).await;
    let (started_status, started_body) = post_internal_event(
        state.clone(),
        json!({
            "session_id": session_id,
            "turn_id": turn_id,
            "type": "turn.started",
            "data": {
                "runtime_instance_id": "rtinst_a",
                "previous_leaf_id": "previous",
                "topology_context": { "entries": [
                    {"id": "previous", "kind": "model_change"}
                ] }
            }
        }),
    )
    .await;
    assert_eq!(started_status, StatusCode::OK, "{started_body:?}");

    fs::write(
        &transcript,
        concat!(
            "{\"type\":\"model_change\",\"id\":\"previous\",\"parentId\":null}\n",
            "{\"type\":\"message\",\"id\":\"user\",\"parentId\":\"previous\",\"message\":{\"role\":\"user\",\"content\":\"question\"}}\n",
            "{\"type\":\"message\",\"id\":\"answer\",\"parentId\":\"user\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"answer\"}]}}\n",
        ),
    )
    .unwrap();
    sqlx::query(
        "UPDATE runtime_bindings SET runtime_instance_id = 'rtinst_b' WHERE session_id = ?",
    )
    .bind(session_id)
    .execute(&state.db())
    .await
    .unwrap();

    let (output_status, output_body) = post_internal_event(
        state.clone(),
        json!({
            "session_id": session_id,
            "turn_id": turn_id,
            "type": "turn.output",
            "data": { "output_summary": "answer" }
        }),
    )
    .await;
    assert_eq!(output_status, StatusCode::OK, "{output_body:?}");

    let (completed_status, completed_body) = post_internal_event(
        state.clone(),
        json!({
            "session_id": session_id,
            "turn_id": turn_id,
            "type": "turn.completed",
            "data": {
                "runtime_instance_id": "rtinst_a",
                "terminal_leaf_id": "answer"
            }
        }),
    )
    .await;
    assert_eq!(completed_status, StatusCode::OK, "{completed_body:?}");

    let (timeline_status, timeline_body) = get_json(
        state,
        &format!("/external/v1/sessions/{session_id}/turns/timeline?direction=backward"),
    )
    .await;
    assert_eq!(timeline_status, StatusCode::OK, "{timeline_body:?}");
    assert_eq!(timeline_body["data"]["items"].as_array().unwrap().len(), 2);
    assert_eq!(
        timeline_body["data"]["items"][0]["content_preview"],
        "question"
    );
    assert_eq!(
        timeline_body["data"]["items"][1]["content_preview"],
        "answer"
    );
}

#[tokio::test]
async fn turn_timeline_reads_sealed_pi_ranges_and_pages_by_turn_id() {
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
                "{\"type\":\"message\",\"id\":\"answer\",\"parentId\":\"user\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"partial answer\"},{\"type\":\"toolCall\",\"id\":\"call_read\",\"name\":\"read\",\"arguments\":{\"path\":\"README.md\"}}]}}\n"
            )
            .as_bytes(),
        )
        .unwrap();
    let (status, growing) = get_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/turns/timeline?direction=forward&turn_id=turn_active&limit=1"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{growing:?}");
    assert_eq!(growing["data"]["items"].as_array().unwrap().len(), 3);
    assert_eq!(
        growing["data"]["items"][1]["content_preview"],
        "partial answer"
    );
    assert_eq!(growing["data"]["items"][2]["kind"], "tool_call");
    assert_eq!(
        growing["data"]["items"][2]["managed_tool_use"]["tool_name"],
        "read"
    );
    let (status, growing_tree) = get_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/turns/tree/updates?from_turn_id=turn_active"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{growing_tree:?}");
    assert_eq!(growing_tree["data"]["groups"][0]["turn_id"], "turn_active");
    assert_eq!(
        growing_tree["data"]["groups"][0]["items"]
            .as_array()
            .unwrap()
            .len(),
        3
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
            concat!(
                "{\"type\":\"message\",\"id\":\"tool_result\",\"parentId\":\"answer\",\"message\":{\"role\":\"toolResult\",\"toolCallId\":\"call_read\",\"toolName\":\"read\",\"content\":[{\"type\":\"text\",\"text\":\"README contents\"}],\"isError\":false}}\n",
                "{\"type\":\"message\",\"id\":\"final\",\"parentId\":\"tool_result\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"final answer\"}]}}\n"
            ).as_bytes(),
        )
        .unwrap();
    let (status, grown) = get_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/turns/timeline?direction=forward&turn_id=turn_active&limit=1"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{grown:?}");
    assert_eq!(grown["data"]["items"].as_array().unwrap().len(), 5);
    assert_eq!(grown["data"]["items"][3]["kind"], "tool_result");
    assert_eq!(grown["data"]["items"][4]["content_preview"], "final answer");
    let (status, grown_tree) = get_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/turns/tree/updates?from_turn_id=turn_active"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{grown_tree:?}");
    assert_eq!(
        grown_tree["data"]["groups"][0]["items"]
            .as_array()
            .unwrap()
            .len(),
        5
    );
    assert_eq!(
        grown_tree["data"]["groups"][0]["items"][4]["content_preview"],
        "final answer"
    );
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
    _event_id: &str,
    event_type: &str,
    timeline_anchor: Value,
) {
    if event_type == "turn.started" {
        precreate_turn_if_missing(&state, session_id, turn_id).await;
    }
    let (status, body) = post_internal_event(
        state,
        json!({
            "session_id": session_id,
            "turn_id": turn_id,
            "type": event_type,
            "data": {
                "runtime_instance_id": "rtinst_projected_timeline",
                "timeline_anchor": timeline_anchor,
            }
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");
}

fn pi_text_turn_entries(
    user_id: &str,
    parent_id: Option<&str>,
    user_content: &str,
    assistant_id: &str,
    assistant_content: &str,
) -> String {
    let user = json!({
        "type": "message",
        "id": user_id,
        "parentId": parent_id,
        "message": { "role": "user", "content": user_content },
    });
    let assistant = json!({
        "type": "message",
        "id": assistant_id,
        "parentId": user_id,
        "message": {
            "role": "assistant",
            "content": [{ "type": "text", "text": assistant_content }],
        },
    });
    format!("{user}\n{assistant}\n")
}

#[tokio::test]
async fn pi_hook_context_projects_a_replayable_conversation_tree_without_persisting_native_evidence()
 {
    let _guard = PI_AGENT_DIR_ENV_LOCK.lock().await;
    let temp = tempdir().unwrap();
    let agent_dir = temp.path().join("agent");
    unsafe { std::env::set_var("PI_AGENT_DIR", &agent_dir) };

    let state = test_state().await;
    let session_id = "sess_pi_linear_topology";
    let session_key = "pi-linear-topology";
    let cwd = temp.path().join("workspace");
    fs::create_dir_all(&cwd).unwrap();
    let cwd = cwd.canonicalize().unwrap();
    seed_session(&state, session_id).await;
    let session_dir = pi_session_dir(&agent_dir, &cwd);
    fs::create_dir_all(&session_dir).unwrap();
    let transcript = session_dir.join(format!("2026-07-16T00-00-00-000Z_{session_key}.jsonl"));
    fs::write(&transcript, b"").unwrap();
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

    let turns = [
        (
            "turn_pi_linear_1",
            "evt_pi_linear_1",
            Value::Array(vec![]),
            None,
        ),
        (
            "turn_pi_linear_2",
            "evt_pi_linear_2",
            json!([
                {"id": "user_1", "kind": "user_message"},
                {"id": "assistant_1", "kind": "assistant_message"}
            ]),
            Some("assistant_1"),
        ),
        (
            "turn_pi_linear_3",
            "evt_pi_linear_3",
            json!([
                {"id": "user_1", "kind": "user_message"},
                {"id": "assistant_1", "kind": "assistant_message"},
                {"id": "model_2", "kind": "model_change"},
                {"id": "user_2", "kind": "user_message"},
                {"id": "assistant_2", "kind": "assistant_message"}
            ]),
            Some("assistant_2"),
        ),
    ];

    for (index, (turn_id, _event_prefix, entries, previous_leaf_id)) in turns.iter().enumerate() {
        precreate_turn_if_missing(&state, session_id, turn_id).await;
        let started = json!({
            "session_id": session_id,
            "turn_id": turn_id,
            "type": "turn.started",
            "data": {
                "runtime_instance_id": "rtinst_pi_linear",
                "timeline_anchor": { "previous_leaf_id": previous_leaf_id },
                "topology_context": { "entries": entries },
            }
        });
        let (status, body) = post_internal_event(state.clone(), started.clone()).await;
        assert_eq!(status, StatusCode::OK, "{body:?}");
        let user_id = format!("user_{}", index + 1);
        let assistant_id = format!("assistant_{}", index + 1);
        fs::write(
            &transcript,
            (1..=index + 1)
                .map(|number| {
                    let user_id = format!("user_{number}");
                    let assistant_id = format!("assistant_{number}");
                    let parent_id = (number > 1).then(|| format!("assistant_{}", number - 1));
                    pi_text_turn_entries(
                        &user_id,
                        parent_id.as_deref(),
                        &format!("question {number}"),
                        &assistant_id,
                        &format!("answer {number}"),
                    )
                })
                .collect::<String>(),
        )
        .unwrap();
        let completed = json!({
            "session_id": session_id,
            "turn_id": turn_id,
            "type": "turn.completed",
            "data": {
                "runtime_instance_id": "rtinst_pi_linear",
                "timeline_anchor": { "terminal_leaf_id": assistant_id },
                "debug_content": user_id,
            }
        });
        assert_eq!(
            post_internal_event(state.clone(), completed).await.0,
            StatusCode::OK
        );
    }

    let branch_turns = [
        (
            "turn_pi_linear_4",
            "evt_pi_linear_4",
            json!([
                {"id": "user_1", "kind": "user_message"},
                {"id": "assistant_1", "kind": "assistant_message"}
            ]),
            "assistant_1",
            "assistant_1",
            "user_4",
            "assistant_4",
        ),
        (
            "turn_pi_linear_5",
            "evt_pi_linear_5",
            json!([
                {"id": "user_1", "kind": "user_message"},
                {"id": "assistant_1", "kind": "assistant_message"},
                {"id": "user_4", "kind": "user_message"},
                {"id": "assistant_4", "kind": "assistant_message"}
            ]),
            "assistant_4",
            "assistant_4",
            "user_5",
            "assistant_5",
        ),
    ];
    for (
        turn_id,
        _event_prefix,
        entries,
        previous_leaf_id,
        native_parent_id,
        user_id,
        assistant_id,
    ) in branch_turns
    {
        precreate_turn_if_missing(&state, session_id, turn_id).await;
        let started = json!({
            "session_id": session_id,
            "turn_id": turn_id,
            "type": "turn.started",
            "data": {
                "runtime_instance_id": "rtinst_pi_linear",
                "timeline_anchor": { "previous_leaf_id": previous_leaf_id },
                "topology_context": { "entries": entries },
            }
        });
        let (status, body) = post_internal_event(state.clone(), started).await;
        assert_eq!(status, StatusCode::OK, "{body:?}");

        let mut transcript_file = fs::OpenOptions::new()
            .append(true)
            .open(&transcript)
            .unwrap();
        transcript_file
            .write_all(
                pi_text_turn_entries(
                    user_id,
                    Some(native_parent_id),
                    &format!("question {user_id}"),
                    assistant_id,
                    &format!("answer {assistant_id}"),
                )
                .as_bytes(),
            )
            .unwrap();
        let completed = json!({
            "session_id": session_id,
            "turn_id": turn_id,
            "type": "turn.completed",
            "data": {
                "runtime_instance_id": "rtinst_pi_linear",
                "timeline_anchor": { "terminal_leaf_id": assistant_id },
            }
        });
        assert_eq!(
            post_internal_event(state.clone(), completed).await.0,
            StatusCode::OK
        );
    }

    let (status, initial_history) = get_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/turns/tree/history?limit=5"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{initial_history:?}");
    assert_eq!(
        initial_history["data"]["groups"]
            .as_array()
            .unwrap()
            .iter()
            .map(|group| group["turn_id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["turn_pi_linear_1", "turn_pi_linear_4", "turn_pi_linear_5"]
    );

    let (status, history) = get_json(
        state.clone(),
        &format!(
            "/external/v1/sessions/{session_id}/turns/tree/history?from_turn_id=turn_pi_linear_5&limit=2"
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{history:?}");
    assert_eq!(
        history["data"]["groups"]
            .as_array()
            .unwrap()
            .iter()
            .map(|group| group["turn_id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["turn_pi_linear_4", "turn_pi_linear_5"]
    );
    assert_eq!(history["data"]["next_from_turn_id"], "turn_pi_linear_1");

    let (status, older_history) = get_json(
        state.clone(),
        &format!(
            "/external/v1/sessions/{session_id}/turns/tree/history?from_turn_id=turn_pi_linear_1&limit=2"
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{older_history:?}");
    assert_eq!(
        older_history["data"]["groups"][0]["turn_id"],
        "turn_pi_linear_1"
    );
    assert!(older_history["data"]["next_from_turn_id"].is_null());

    let (status, updates) = get_json(
        state.clone(),
        &format!(
            "/external/v1/sessions/{session_id}/turns/tree/updates?from_turn_id=turn_pi_linear_3"
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{updates:?}");
    assert_eq!(updates["data"]["current_turn_id"], "turn_pi_linear_5");
    assert_eq!(
        updates["data"]["retain_through_turn_id"],
        "turn_pi_linear_1"
    );
    assert_eq!(
        updates["data"]["groups"]
            .as_array()
            .unwrap()
            .iter()
            .map(|group| group["turn_id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["turn_pi_linear_4", "turn_pi_linear_5"]
    );

    let (status, inclusive_updates) = get_json(
        state.clone(),
        &format!(
            "/external/v1/sessions/{session_id}/turns/tree/updates?from_turn_id=turn_pi_linear_4"
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{inclusive_updates:?}");
    assert_eq!(
        inclusive_updates["data"]["retain_through_turn_id"],
        "turn_pi_linear_4"
    );
    assert_eq!(
        inclusive_updates["data"]["groups"]
            .as_array()
            .unwrap()
            .iter()
            .map(|group| group["turn_id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["turn_pi_linear_4", "turn_pi_linear_5"]
    );

    precreate_turn_if_missing(&state, session_id, "turn_pi_linear_6").await;
    let disconnected_started = json!({
        "session_id": session_id,
        "turn_id": "turn_pi_linear_6",
        "type": "turn.started",
        "data": {
            "runtime_instance_id": "rtinst_pi_linear",
            "timeline_anchor": { "previous_leaf_id": "assistant_5" },
            "topology_context": { "entries": [] },
        }
    });
    assert_eq!(
        post_internal_event(state.clone(), disconnected_started)
            .await
            .0,
        StatusCode::OK
    );
    fs::OpenOptions::new()
        .append(true)
        .open(&transcript)
        .unwrap()
        .write_all(
            pi_text_turn_entries(
                "user_6",
                Some("assistant_5"),
                "disconnected question",
                "assistant_6",
                "disconnected answer",
            )
            .as_bytes(),
        )
        .unwrap();
    let disconnected_completed = json!({
        "session_id": session_id,
        "turn_id": "turn_pi_linear_6",
        "type": "turn.completed",
        "data": {
            "runtime_instance_id": "rtinst_pi_linear",
            "timeline_anchor": { "terminal_leaf_id": "assistant_6" },
        }
    });
    assert_eq!(
        post_internal_event(state.clone(), disconnected_completed)
            .await
            .0,
        StatusCode::OK
    );

    let (status, disconnected_updates) = get_json(
        state.clone(),
        &format!(
            "/external/v1/sessions/{session_id}/turns/tree/updates?from_turn_id=turn_pi_linear_5"
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{disconnected_updates:?}");
    assert!(disconnected_updates["data"]["retain_through_turn_id"].is_null());
    assert_eq!(
        disconnected_updates["data"]["groups"][0]["turn_id"],
        "turn_pi_linear_6"
    );

    precreate_turn_if_missing(&state, session_id, "turn_pi_linear_malformed").await;
    let malformed_started = json!({
        "session_id": session_id,
        "turn_id": "turn_pi_linear_malformed",
        "type": "turn.started",
        "data": {
            "runtime_instance_id": "rtinst_pi_linear",
            "timeline_anchor": { "previous_leaf_id": "assistant_6" },
            "topology_context": { "entries": [
                {"id": "native-secret-entry", "kind": "user_message"},
                {"id": "native-secret-entry", "kind": "assistant_message"}
            ] },
        }
    });
    let captured_logs = CapturedLogWriter::default();
    let subscriber = tracing_subscriber::fmt()
        .json()
        .without_time()
        .with_writer(captured_logs.clone())
        .finish();
    let (malformed_status, malformed_body) = post_internal_event(state.clone(), malformed_started)
        .with_subscriber(subscriber)
        .await;
    assert_eq!(malformed_status, StatusCode::OK, "{malformed_body:?}");
    let log_text = captured_logs.text();
    let warning = log_text
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .find(|entry| entry["fields"]["code"] == "turn_topology_unresolved")
        .expect("structured topology warning");
    assert_eq!(warning["fields"]["diagnostic"], "evidence_invalid");
    assert!(!log_text.contains("native-secret-entry"));
    let turn_five = EventIngestService::new(state.db())
        .get_turn("turn_pi_linear_5")
        .await
        .unwrap()
        .unwrap();
    assert!(!log_text.contains(turn_five.tail_cursor.as_deref().unwrap()));

    let (status, body) = get_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/turns"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");
    let projected = body["data"]["turns"].as_array().unwrap();
    assert_eq!(projected.len(), 7);
    assert!(
        projected
            .iter()
            .all(|turn| turn.get("turn_index").is_none())
    );
    assert_eq!(projected[0]["topology_status"], "root");
    assert_eq!(projected[1]["parent_turn_id"], "turn_pi_linear_1");
    assert_eq!(projected[2]["parent_turn_id"], "turn_pi_linear_2");
    assert_eq!(projected[3]["parent_turn_id"], "turn_pi_linear_1");
    assert_eq!(projected[4]["parent_turn_id"], "turn_pi_linear_4");
    assert_eq!(projected[5]["topology_status"], "root");
    assert_eq!(projected[6]["topology_status"], "unknown");
    assert_eq!(projected[6]["state"], "running");

    let (status, unknown_updates) = get_json(
        state.clone(),
        &format!(
            "/external/v1/sessions/{session_id}/turns/tree/updates?from_turn_id=turn_pi_linear_5"
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{unknown_updates:?}");
    assert_eq!(unknown_updates["error"]["code"], "turn_topology_unknown");

    for (selected_turn_id, expected_preview) in [
        ("turn_pi_linear_3", "question 3"),
        ("turn_pi_linear_4", "question user_4"),
    ] {
        let (status, selected) = get_json(
            state.clone(),
            &format!(
                "/external/v1/sessions/{session_id}/turns/timeline?direction=forward&turn_id={selected_turn_id}&limit=1"
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{selected:?}");
        let items = selected["data"]["items"].as_array().unwrap();
        assert!(!items.is_empty());
        assert!(items.iter().all(|item| item["turn_id"] == selected_turn_id));
        assert_eq!(items[0]["content_preview"], expected_preview);
    }

    let events = EventIngestService::new(state.db())
        .list_events(session_id)
        .await
        .unwrap();
    let started_events: Vec<_> = events
        .iter()
        .filter(|event| event.event_type == EventType::TurnStarted)
        .collect();
    assert_eq!(started_events.len(), 7);
    assert!(started_events.iter().all(|event| {
        event.payload.get("topology_context").is_none()
            && event.payload.get("timeline_anchor").is_none()
    }));
    assert!(started_events.iter().all(|event| event.topology.is_some()));

    fs::remove_file(&transcript).unwrap();
    let mut replay = ProjectionState::default();
    for event in &events {
        replay.apply(event).unwrap();
    }
    assert_eq!(
        replay
            .turn("turn_pi_linear_3")
            .unwrap()
            .topology
            .parent_turn_id(),
        Some("turn_pi_linear_2")
    );
    assert_eq!(
        replay
            .turn("turn_pi_linear_5")
            .unwrap()
            .topology
            .parent_turn_id(),
        Some("turn_pi_linear_4")
    );
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

    precreate_turn_if_missing(&state, session_id, turn_id).await;
    let started = json!({
        "session_id": session_id,
        "turn_id": turn_id,
        "type": "turn.started",
        "data": {
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
        "session_id": session_id,
        "turn_id": turn_id,
        "type": "turn.completed",
        "data": {
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
    assert!(body["data"]["turn"].get("turn_index").is_none());
    assert!(body["data"]["turn"].get("head_cursor").is_none());
    assert!(body["data"]["turn"].get("tail_cursor").is_none());

    let events = EventIngestService::new(state.db())
        .list_events(session_id)
        .await
        .unwrap();
    let started_event = events
        .iter()
        .find(|event| event.event_type == EventType::TurnStarted)
        .expect("started event");
    assert!(started_event.payload.get("timeline_anchor").is_none());
    assert_eq!(
        started_event.timeline_boundary,
        Some(TimelineBoundary::head(expected_head.clone()))
    );
    assert!(started_event.payload.get("timeline_boundary").is_none());
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
async fn interrupted_pi_turn_captures_tail_boundary_and_remains_timeline_readable() {
    let _guard = PI_AGENT_DIR_ENV_LOCK.lock().await;
    let temp = tempdir().unwrap();
    let agent_dir = temp.path().join("agent");
    unsafe { std::env::set_var("PI_AGENT_DIR", &agent_dir) };

    let state = test_state().await;
    let session_id = "sess_pi_interrupted_boundary";
    let turn_id = "turn_pi_interrupted_boundary";
    let session_key = "pi-interrupted-boundary";
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

    precreate_turn_if_missing(&state, session_id, turn_id).await;
    let (status, body) = post_internal_event(
        state.clone(),
        json!({
            "session_id": session_id,
            "turn_id": turn_id,
            "type": "turn.started",
            "data": {
                "runtime_instance_id": "rtinst_pi_interrupted_boundary",
                "previous_leaf_id": "previous_leaf"
            }
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");

    fs::OpenOptions::new()
        .append(true)
        .open(&transcript)
        .unwrap()
        .write_all(
            concat!(
                "{\"type\":\"message\",\"id\":\"user_leaf\",\"parentId\":\"previous_leaf\",\"message\":{\"role\":\"user\",\"content\":\"question\"}}\n",
                "{\"type\":\"message\",\"id\":\"terminal_leaf\",\"parentId\":\"user_leaf\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"partial answer\"}],\"stopReason\":\"aborted\"}}\n"
            )
            .as_bytes(),
        )
        .unwrap();
    let tail_offset = fs::metadata(&transcript).unwrap().len();

    let (status, body) = post_internal_event(
        state.clone(),
        json!({
            "session_id": session_id,
            "turn_id": turn_id,
            "type": "turn.interrupted",
            "data": {
                "runtime_instance_id": "rtinst_pi_interrupted_boundary",
                "terminal_leaf_id": "terminal_leaf"
            }
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");

    let (status, body) = get_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/turns/timeline?direction=backward"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");
    assert_eq!(body["data"]["items"].as_array().unwrap().len(), 2);

    let expected_head = format!(
        "pi-jsonl-v2:{}:{head_offset}:after:previous_leaf",
        binding.id
    );
    let expected_tail = format!(
        "pi-jsonl-v2:{}:{tail_offset}:after:terminal_leaf",
        binding.id
    );
    let turn = EventIngestService::new(state.db())
        .get_turn(turn_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(turn.head_cursor.as_deref(), Some(expected_head.as_str()));
    assert_eq!(turn.tail_cursor.as_deref(), Some(expected_tail.as_str()));

    let events = EventIngestService::new(state.db())
        .list_events(session_id)
        .await
        .unwrap();
    let interrupted = events
        .iter()
        .find(|event| event.event_type == EventType::TurnInterrupted)
        .expect("interrupted event");
    assert_eq!(
        interrupted.timeline_boundary,
        Some(TimelineBoundary::tail(expected_tail))
    );
    assert!(interrupted.payload.get("timeline_anchor").is_none());
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
        .ingest_reported_event(ReportedEvent::new(
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

    precreate_turn_if_missing(&state, session_id, turn_id).await;
    let started = json!({
        "session_id": session_id,
        "turn_id": turn_id,
        "type": "turn.started",
        "data": {
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
async fn timeline_capture_failure_keeps_lifecycle_fact_and_logs_structured_warning() {
    let _guard = PI_AGENT_DIR_ENV_LOCK.lock().await;
    let temp = tempdir().unwrap();
    let agent_dir = temp.path().join("missing-agent-dir");
    unsafe { std::env::set_var("PI_AGENT_DIR", &agent_dir) };
    let state = test_state().await;
    let session_id = "sess_pi_boundary_missing";
    seed_session(&state, session_id).await;
    EventIngestService::new(state.db())
        .ingest_reported_event(ReportedEvent::new(
            "evt_existing_created".to_string(),
            session_id.to_string(),
            Some("turn_existing".to_string()),
            EventSource::ExternalApi,
            "pi".to_string(),
            EventType::TurnCreated,
            json!({}),
        ))
        .await
        .unwrap();
    EventIngestService::new(state.db())
        .ingest_reported_event(ReportedEvent::new(
            "evt_existing_completed".to_string(),
            session_id.to_string(),
            Some("turn_existing".to_string()),
            EventSource::ExternalApi,
            "pi".to_string(),
            EventType::TurnCompleted,
            json!({}),
        ))
        .await
        .unwrap();
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
    precreate_turn_if_missing(&state, session_id, "turn_pi_boundary_missing").await;
    let started = json!({
        "session_id": session_id,
        "turn_id": "turn_pi_boundary_missing",
        "type": "turn.started",
        "data": {
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
    let warning = captured_logs
        .text()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .find(|entry| entry["fields"]["code"] == "timeline_boundary_capture_failed")
        .expect("structured timeline capture warning");
    assert_eq!(warning["level"], "WARN");
    assert!(
        warning["fields"]["event_id"]
            .as_str()
            .is_some_and(|event_id| event_id.starts_with("evt_"))
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
