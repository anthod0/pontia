use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
    routing::get,
};
use http_body_util::BodyExt;
use tower::ServiceExt;

use pontia::transport::http::{
    self,
    dashboard::{ResolvedDashboard, resolve_dashboard},
};
use pontia::{application::AppState, config::DashboardConfig};
use pontia_storage_sqlite::{connect_sqlite, run_migrations};

async fn test_state_with_dashboard(dashboard: ResolvedDashboard) -> http::HttpState {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("dashboard.db");
    let _kept_dir = dir.keep();
    let database_url = format!("sqlite://{}", db_path.display());
    let db = connect_sqlite(&database_url).await.expect("connect");
    run_migrations(&db).await.expect("migrate");
    let app_state = AppState::builder(db)
        .external_api_token(Some("test-token".to_owned()))
        .build();
    http::HttpState::new(app_state, dashboard)
}

#[tokio::test]
async fn dashboard_serves_configured_local_entrypoint() {
    let (_dir, root) = build_local_dashboard("custom dashboard", "custom.js");

    let response =
        http::router(test_state_with_dashboard(ResolvedDashboard::available(root.clone())).await)
            .oneshot(
                Request::builder()
                    .uri("/dashboard")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    let html = std::str::from_utf8(&body).expect("utf8 html");
    assert!(html.contains("custom dashboard"));

    let response =
        http::router(test_state_with_dashboard(ResolvedDashboard::available(root)).await)
            .oneshot(
                Request::builder()
                    .uri("/dashboard/assets/custom.js")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("asset response");

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn dashboard_serves_files_from_configured_dist_root() {
    let (_dir, root) = build_local_dashboard("custom dashboard", "custom.js");
    let logo_dir = root.join("logo");
    std::fs::create_dir_all(&logo_dir).expect("logo dir");
    std::fs::write(logo_dir.join("mark.png"), b"png-bytes").expect("logo file");

    let response =
        http::router(test_state_with_dashboard(ResolvedDashboard::available(root)).await)
            .oneshot(
                Request::builder()
                    .uri("/dashboard/logo/mark.png")
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
    assert_eq!(content_type, "image/png");
    let body = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    assert_eq!(&body[..], b"png-bytes");
}

#[tokio::test]
async fn dashboard_spa_fallback_serves_entrypoint_for_nested_routes() {
    let (_dir, root) = build_local_dashboard("custom dashboard", "custom.js");

    for path in [
        "/dashboard/",
        "/dashboard/overview",
        "/dashboard/tasks/example/dag",
    ] {
        let response = http::router(
            test_state_with_dashboard(ResolvedDashboard::available(root.clone())).await,
        )
        .oneshot(
            Request::builder()
                .uri(path)
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

        assert_eq!(response.status(), StatusCode::OK, "{path}");
        let body = response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        let html = std::str::from_utf8(&body).expect("utf8 html");
        assert!(html.contains("custom dashboard"), "{path}");
    }
}

#[tokio::test]
async fn dashboard_reports_unavailable_remote_without_cache() {
    let (status, message) = request_dashboard(ResolvedDashboard::unavailable(
        "failed to refresh remote dashboard".to_string(),
    ))
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(message.contains("failed to refresh remote dashboard"));
}

#[tokio::test]
async fn missing_dashboard_config_reports_that_dashboard_is_not_configured() {
    let dashboard = resolve_dashboard(&DashboardConfig::default()).await;
    let (status, message) = request_dashboard(dashboard).await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(message.contains("Dashboard source is not configured"));
}

#[tokio::test]
async fn configured_local_dashboard_without_index_reports_missing_entrypoint() {
    let dir = tempfile::tempdir().expect("tempdir");
    let dashboard = resolve_dashboard(&DashboardConfig {
        source: Some(dir.path().display().to_string()),
        cache_dir: None,
    })
    .await;
    let (status, message) = request_dashboard(dashboard).await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(message.contains("dashboard entrypoint not found"));
}

#[tokio::test]
async fn remote_dashboard_refreshes_cache_and_falls_back_when_refresh_fails() {
    let archive = build_dashboard_zip("remote dashboard", "remote.js");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind archive server");
    let addr = listener.local_addr().expect("local addr");
    let server = tokio::spawn(async move {
        let app = Router::new().route(
            "/dashboard.zip",
            get(move || {
                let archive = archive.clone();
                async move { ([(header::CONTENT_TYPE, "application/zip")], archive) }
            }),
        );
        axum::serve(listener, app).await.expect("archive server");
    });

    let cache_dir = tempfile::tempdir().expect("cache dir");
    let config = DashboardConfig {
        source: Some(format!("http://{addr}/dashboard.zip")),
        cache_dir: Some(cache_dir.path().display().to_string()),
    };
    let dashboard = resolve_dashboard(&config).await;
    let html = request_dashboard_html(dashboard).await;
    assert!(html.contains("remote dashboard"));

    server.abort();

    let dashboard = resolve_dashboard(&config).await;
    let html = request_dashboard_html(dashboard).await;
    assert!(html.contains("remote dashboard"));
}

#[tokio::test]
async fn dashboard_serves_built_svelte_entrypoint() {
    let dashboard = dashboard_v2_dist().await;
    let response = http::router(test_state_with_dashboard(dashboard).await)
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

    assert!(html.contains("pontia Dashboard"));
    assert!(html.contains("id=\"app\""));
    assert!(html.contains("/dashboard/assets/"));
    assert!(!html.contains("openEventStream"));
}

async fn request_dashboard_html(dashboard: ResolvedDashboard) -> String {
    let (status, html) = request_dashboard(dashboard).await;
    assert_eq!(status, StatusCode::OK);
    html
}

async fn request_dashboard(dashboard: ResolvedDashboard) -> (StatusCode, String) {
    let response = http::router(test_state_with_dashboard(dashboard).await)
        .oneshot(
            Request::builder()
                .uri("/dashboard")
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
    (
        status,
        std::str::from_utf8(&body).expect("utf8 html").to_string(),
    )
}

async fn dashboard_v2_dist() -> ResolvedDashboard {
    resolve_dashboard(&DashboardConfig {
        source: Some("apps/dashboard/dist".to_string()),
        cache_dir: None,
    })
    .await
}

fn build_local_dashboard(
    html_text: &str,
    script_name: &str,
) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().join("custom-dashboard");
    let assets = root.join("assets");
    std::fs::create_dir_all(&assets).expect("assets dir");
    std::fs::write(
        root.join("index.html"),
        format!(
            r#"<html><head><script src="/dashboard/assets/{script_name}"></script></head><body>{html_text}</body></html>"#
        ),
    )
    .expect("index");
    std::fs::write(assets.join(script_name), "console.log('custom');").expect("asset");
    (dir, root)
}

fn build_dashboard_zip(html_text: &str, script_name: &str) -> Vec<u8> {
    let mut bytes = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut bytes);
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("dist/index.html", options)
            .expect("index entry");
        std::io::Write::write_all(
            &mut zip,
            format!(
                r#"<html><head><script src="/dashboard/assets/{script_name}"></script></head><body>{html_text}</body></html>"#
            )
            .as_bytes(),
        )
        .expect("index body");
        zip.start_file(format!("dist/assets/{script_name}"), options)
            .expect("asset entry");
        std::io::Write::write_all(&mut zip, b"console.log('remote');").expect("asset body");
        zip.finish().expect("finish zip");
    }
    bytes.into_inner()
}

#[tokio::test]
async fn dashboard_serves_built_frontend_assets() {
    let dashboard = dashboard_v2_dist().await;
    let entry_response = http::router(test_state_with_dashboard(dashboard).await)
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

    let dashboard = dashboard_v2_dist().await;
    let response = http::router(test_state_with_dashboard(dashboard).await)
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
