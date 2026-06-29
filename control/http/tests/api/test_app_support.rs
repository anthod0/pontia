use crate::test_app::TestApp;
use pontia_application::EventIngestService;
use pontia_core::domain::{DomainEvent, EventSource, EventType};
use serde_json::json;
use sqlx::query_scalar;

#[tokio::test]
async fn test_app_creates_isolated_state_home_and_workspace() {
    let app = TestApp::builder()
        .external_api_token(Some("token".to_string()))
        .pi_runtime_stub(true)
        .build()
        .await;

    assert_eq!(app.state.external_api_token(), Some("token"));
    assert!(
        app.pontia_home()
            .path()
            .join("state")
            .starts_with(app.pontia_home().path())
    );
    assert!(app.workspace().path().exists());
    assert_eq!(
        std::env::var("PONTIA_HOME").ok().as_deref(),
        Some(app.pontia_home().path().to_str().unwrap())
    );
    assert_eq!(
        std::env::var("PONTIA_PI_TUI_COMMAND").ok().as_deref(),
        Some("sh -c 'cat >> \"$PONTIA_WORKSPACE/pi-tui-input.log\"' --")
    );
}

#[tokio::test]
async fn build_state_remains_usable_after_helper_returns() {
    let state = TestApp::builder()
        .external_api_token(None)
        .build_state()
        .await;

    let count: i64 = query_scalar("SELECT COUNT(*) FROM tasks")
        .fetch_one(&state.db())
        .await
        .expect("query migrated database");
    assert_eq!(count, 0);
}

#[tokio::test]
async fn build_state_supports_event_projection_updates() {
    let state = TestApp::builder().build_state().await;
    EventIngestService::new(state.db())
        .ingest_event(DomainEvent::new(
            "evt_test_app_projection".to_string(),
            "sess_test_app_projection".to_string(),
            None,
            EventSource::AgentAdapter,
            "generic".to_string(),
            EventType::SessionCreated,
            json!({}),
        ))
        .await
        .expect("ingest event");

    let count: i64 =
        query_scalar("SELECT COUNT(*) FROM sessions WHERE session_id = 'sess_test_app_projection'")
            .fetch_one(&state.db())
            .await
            .expect("query projection");
    assert_eq!(count, 1);
}
