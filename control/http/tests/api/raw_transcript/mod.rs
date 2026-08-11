use crate::test_app::TestApp;
use std::{
    fs,
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
    sync::{Arc, Mutex as StdMutex},
};

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use pontia_agent_clients::pi::raw_transcripts::{PiJsonlV2Cursor, TimelineBoundaryRelation};
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

mod branch_replay;
mod timeline_boundaries;
mod timeline_queries;
mod timeline_reading;
mod turn_tree;

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
    post_internal_json(state, "/internal/v1/events", body).await
}

async fn post_internal_json(state: AppState, uri: &str, body: Value) -> (StatusCode, Value) {
    let response = http::router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
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

async fn post_external_json(
    state: AppState,
    uri: &str,
    idempotency_key: Option<&str>,
    body: Value,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method("POST")
        .uri(uri)
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
        .header(header::CONTENT_TYPE, "application/json");
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
            client_session_file: Some(transcript.display().to_string()),
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
