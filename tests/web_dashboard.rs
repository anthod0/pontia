use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use tower::ServiceExt;

use llmparty::{
    application::AppState,
    storage::sqlite::{connect_sqlite, run_migrations},
    transport::http,
};

async fn test_state() -> AppState {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("dashboard.db");
    let _kept_dir = dir.keep();
    let database_url = format!("sqlite://{}", db_path.display());
    let db = connect_sqlite(&database_url).await.expect("connect");
    run_migrations(&db).await.expect("migrate");
    AppState {
        db,
        external_api_token: Some("test-token".to_owned()),
    }
}

#[tokio::test]
async fn dashboard_serves_minimal_web_control_panel() {
    let response = http::router(test_state().await)
        .oneshot(
            Request::builder()
                .uri("/dashboard")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);

    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .expect("content-type");
    assert!(content_type.starts_with("text/html"));

    let body = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    let html = std::str::from_utf8(&body).expect("utf8 html");

    assert!(html.contains("llmparty Dashboard"));
    assert!(html.contains("API token"));
    assert!(html.contains("Create session"));
    assert!(html.contains("Submit turn"));
    assert!(html.contains("Event timeline"));
    assert!(html.contains("Artifact browser"));
    assert!(html.contains("/external/v1/sessions"));
    assert!(html.contains("localStorage"));
}
