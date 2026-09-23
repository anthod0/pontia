use crate::common::test_app::TestApp;
use pontia_application::{AppState, EventIngestService};
use pontia_core::{
    domain::{EventSource, EventType, ReportedEvent},
    ids::new_event_id,
};
use pontia_storage_sqlite::repositories::runtime_bindings::{
    RuntimeBindingUpsertRecord, SqliteRuntimeBindingRepository,
};
use serde_json::json;

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
