use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use pontia_application::AppState;
use serde_json::json;
use tower::ServiceExt;

#[tokio::test]
async fn history_changes_use_the_dashboard_stream_without_persisting_execution_facts() {
    let root = tempfile::tempdir().unwrap();
    let pool = pontia_storage_sqlite::connect_sqlite("sqlite::memory:")
        .await
        .unwrap();
    pontia_storage_sqlite::run_migrations(&pool).await.unwrap();
    let app = AppState::builder(
        pontia_agent_control::AgentControl::builder(pool, root.path().into()).build(),
    )
    .external_api_token(Some("test".into()))
    .build();
    let response = pontia_http::router(app.clone())
        .oneshot(
            Request::builder()
                .uri("/api/v1/dashboard/events/stream")
                .header("authorization", "Bearer test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let mut body = response.into_body();
    app.control()
        .event_ingest_service()
        .history_changed("session");
    let frame = tokio::time::timeout(std::time::Duration::from_secs(3), body.frame())
        .await
        .unwrap()
        .unwrap()
        .unwrap()
        .into_data()
        .unwrap();
    let frame = std::str::from_utf8(&frame).unwrap();
    let data = frame
        .lines()
        .find_map(|line| line.strip_prefix("data: "))
        .unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(data).unwrap(),
        json!({"kind":"history_changed","session_id":"session"})
    );
    assert!(!frame.lines().any(|line| line.starts_with("id:")));
    let events: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events")
        .fetch_one(&app.db())
        .await
        .unwrap();
    assert_eq!(events, 0);
}
