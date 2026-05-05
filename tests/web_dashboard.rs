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
        planner: Default::default(),
    }
}

#[tokio::test]
async fn dashboard_serves_built_svelte_entrypoint() {
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
    assert!(html.contains("id=\"app\""));
    assert!(html.contains("/dashboard/assets/"));
    assert!(!html.contains("openEventStream"));
}

#[tokio::test]
async fn dashboard_serves_built_frontend_assets() {
    let entry_response = http::router(test_state().await)
        .oneshot(
            Request::builder()
                .uri("/dashboard")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("entry response");
    let entry_body = entry_response
        .into_body()
        .collect()
        .await
        .expect("entry body")
        .to_bytes();
    let html = std::str::from_utf8(&entry_body).expect("utf8 html");
    let asset_start = html.find("/dashboard/assets/").expect("asset path");
    let asset_end = html[asset_start..]
        .find('"')
        .map(|offset| asset_start + offset)
        .expect("asset end");
    let asset_path = &html[asset_start..asset_end];

    let response = http::router(test_state().await)
        .oneshot(
            Request::builder()
                .uri(asset_path)
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
    assert!(content_type.contains("javascript"));
}
