use crate::common::test_app::TestApp;
use pontia_application::{AppState, EventIngestService};
use pontia_core::{
    domain::{EventSource, EventType, ReportedEvent},
    ids::new_event_id,
};
use pontia_storage_sqlite::repositories::session_runtimes::{
    SessionRuntimeRecord, SqliteSessionRuntimeRepository,
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
    EventIngestService::for_projection_tests(state.db())
        .with_clients(crate::common::clients::clients())
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

pub(super) async fn bind_runtime(state: &AppState, session_id: &str, runtime_id: &str) {
    sqlx::query("UPDATE sessions SET workspace_ref='/tmp' WHERE session_id=?")
        .bind(session_id)
        .execute(&state.db())
        .await
        .unwrap();
    if let Some(existing) = SqliteSessionRuntimeRepository::new(state.db())
        .runtime_id(session_id)
        .await
        .unwrap()
        && existing != runtime_id
    {
        sqlx::query("UPDATE session_runtimes SET runtime_id=? WHERE runtime_id=?")
            .bind(runtime_id)
            .bind(existing)
            .execute(&state.db())
            .await
            .unwrap();
    }
    SqliteSessionRuntimeRepository::new(state.db())
        .upsert_binding(SessionRuntimeRecord {
            session_id: session_id.to_string(),
            runtime_id: runtime_id.to_string(),
            start_command: None,
            tmux_socket_path: None,
            tmux_pane_id: None,
            process_fingerprint: None,
            role: "tui".into(),
            state: "running".into(),
            created_at: "2026-10-01T00:00:00Z".into(),
        })
        .await
        .expect("bind runtime");
}
