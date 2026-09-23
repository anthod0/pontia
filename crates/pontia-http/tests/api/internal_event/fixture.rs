use crate::common::test_app::TestApp;
use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use pontia_application::{AppState, EventIngestService};
use pontia_core::{
    domain::{EventSource, EventType, ReportedEvent},
    ids::new_event_id,
};
use pontia_http as http;
use pontia_storage_sqlite::repositories::runtime_bindings::{
    RuntimeBindingUpsertRecord, SqliteRuntimeBindingRepository,
};
use serde_json::{Value, json};
use tower::ServiceExt;

pub(super) async fn test_state() -> AppState {
    TestApp::builder()
        .database_name("internal_event.db")
        .external_api_token(None)
        .build_state()
        .await
}

pub(super) async fn create_session(state: &AppState, session_id: &str, client_type: &str) {
    EventIngestService::new(state.db())
        .ingest_reported_event(ReportedEvent::new(
            new_event_id().to_string(),
            session_id.to_string(),
            None,
            EventSource::ExternalApi,
            client_type.to_string(),
            EventType::SessionCreated,
            json!({}),
        ))
        .await
        .expect("create session");
}

pub(super) async fn bind_runtime(state: &AppState, session_id: &str, runtime_instance_id: &str) {
    SqliteRuntimeBindingRepository::new(state.db())
        .upsert_binding(RuntimeBindingUpsertRecord {
            session_id: session_id.to_string(),
            runtime_kind: "tmux".to_string(),
            runtime_instance_id: Some(runtime_instance_id.to_string()),
            binding_state: "confirmed".to_string(),
            runtime_handle: None,
            start_command: None,
            launch_cwd: Some("/tmp".to_string()),
            internal_event_url: None,
            started_at: None,
            last_seen_at: None,
            restart_count: 0,
            tmux_socket_path: None,
            tmux_pane_id: None,
            process_fingerprint: None,
            capabilities: "{}".to_string(),
            diagnostics: "{}".to_string(),
            adapter_details: "{}".to_string(),
        })
        .await
        .expect("bind runtime");
}

pub(super) async fn post_event(state: AppState, body: Value) -> (StatusCode, Value) {
    post_json(state, "/internal/v1/events", body).await
}

pub(super) async fn post_json(state: AppState, path: &str, body: Value) -> (StatusCode, Value) {
    let response = http::router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(path)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .expect("request"),
        )
        .await
        .expect("response");
    let status = response.status();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    (status, serde_json::from_slice(&bytes).expect("json body"))
}
