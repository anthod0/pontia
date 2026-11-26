use super::common::test_app::TestApp;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use tower::ServiceExt;

async fn fixture() -> TestApp {
    let app = TestApp::new().await;
    sqlx::query("INSERT INTO sessions (session_id, client_type, state) VALUES ('s', 'generic', 'idle'), ('other', 'generic', 'idle')")
        .execute(&app.db).await.unwrap();
    app
}

async fn path_content(
    app: &TestApp,
    session: &str,
    path: &std::path::Path,
    authenticated: bool,
) -> axum::response::Response {
    let mut url = reqwest::Url::parse(&format!(
        "http://localhost/api/v1/sessions/{session}/files/content"
    ))
    .unwrap();
    url.query_pairs_mut()
        .append_pair("path", path.to_str().unwrap());
    let mut request = Request::builder().uri(&url.as_str()["http://localhost".len()..]);
    if authenticated {
        request = request.header("Authorization", "Bearer test-token");
    }
    pontia_http::router(app.state.clone())
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap()
}

#[tokio::test]
async fn path_download_reads_current_file_without_creating_events() {
    let app = fixture().await;
    let workspace = tempfile::tempdir().unwrap();
    let path = workspace.path().join("报告 #1.txt");
    for bytes in [b"original".as_slice(), b"updated"] {
        std::fs::write(&path, bytes).unwrap();
        let response = path_content(&app, "s", &path, true).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers()["content-length"],
            bytes.len().to_string()
        );
        assert!(
            response.headers()["content-disposition"]
                .to_str()
                .unwrap()
                .starts_with("attachment; filename*=UTF-8''")
        );
        assert_eq!(response.headers()["x-content-type-options"], "nosniff");
        assert_eq!(response.headers()["cache-control"], "no-store");
        assert_eq!(
            response.into_body().collect().await.unwrap().to_bytes(),
            bytes
        );
    }
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(count, 0);
    std::fs::remove_file(&path).unwrap();
    assert_eq!(
        path_content(&app, "s", &path, true).await.status(),
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn path_download_streams_a_five_mibibyte_preview_without_truncation() {
    let app = fixture().await;
    let workspace = tempfile::tempdir().unwrap();
    let path = workspace.path().join("preview.bin");
    let content = vec![17; 5 * 1024 * 1024];
    std::fs::write(&path, &content).unwrap();

    let response = path_content(&app, "s", &path, true).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()["content-length"],
        content.len().to_string()
    );
    assert_eq!(
        response.into_body().collect().await.unwrap().to_bytes(),
        content
    );
}

#[tokio::test]
async fn path_download_requires_authentication_session_and_absolute_regular_file_within_size_limit()
{
    let app = fixture().await;
    let workspace = tempfile::tempdir().unwrap();
    let path = workspace.path().join("report.txt");
    let file = std::fs::File::create(&path).unwrap();
    assert_eq!(
        path_content(&app, "s", &path, false).await.status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        path_content(&app, "missing", &path, true).await.status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        path_content(&app, "s", std::path::Path::new("relative.txt"), true)
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        path_content(&app, "s", workspace.path(), true)
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    file.set_len((1 << 30) + 1).unwrap();
    assert_eq!(
        path_content(&app, "s", &path, true).await.status(),
        StatusCode::PAYLOAD_TOO_LARGE
    );
}

#[tokio::test]
async fn path_download_resolves_symlinks_and_rejects_non_regular_files() {
    let app = fixture().await;
    let workspace = tempfile::tempdir().unwrap();
    let target = workspace.path().join("target");
    let path = workspace.path().join("link");
    std::fs::write(&target, b"linked file").unwrap();
    std::os::unix::fs::symlink(&target, &path).unwrap();
    let response = path_content(&app, "s", &path, true).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.into_body().collect().await.unwrap().to_bytes(),
        &b"linked file"[..]
    );
    let socket_path = workspace.path().join("socket");
    let _socket = std::os::unix::net::UnixListener::bind(&socket_path).unwrap();
    assert_ne!(
        path_content(&app, "s", &socket_path, true).await.status(),
        StatusCode::OK
    );
}

#[tokio::test]
async fn path_download_types_only_recognized_images() {
    let app = fixture().await;
    let workspace = tempfile::tempdir().unwrap();
    let path = workspace.path().join("图片.svg");
    for (bytes, media_type) in [
        (b"\x89PNG\r\n\x1a\nimage".as_slice(), "image/png"),
        (b"<svg onload='alert(1)'/>", "application/octet-stream"),
    ] {
        std::fs::write(&path, bytes).unwrap();
        let response = path_content(&app, "s", &path, true).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["content-type"], media_type);
    }
}

#[tokio::test]
async fn path_head_reports_size_and_type_without_a_body() {
    let app = fixture().await;
    let workspace = tempfile::tempdir().unwrap();
    let path = workspace.path().join("chart.png");
    std::fs::write(&path, b"\x89PNG\r\n\x1a\nimage bytes").unwrap();
    let head = |path: std::path::PathBuf| {
        let mut url =
            reqwest::Url::parse("http://localhost/api/v1/sessions/s/files/content").unwrap();
        url.query_pairs_mut()
            .append_pair("path", path.to_str().unwrap());
        pontia_http::router(app.state.clone()).oneshot(
            Request::builder()
                .method("HEAD")
                .uri(&url.as_str()["http://localhost".len()..])
                .header("Authorization", "Bearer test-token")
                .body(Body::empty())
                .unwrap(),
        )
    };
    let response = head(path.clone()).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["content-length"], "19");
    assert_eq!(response.headers()["content-type"], "image/png");
    assert!(
        response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .is_empty()
    );
    std::fs::remove_file(&path).unwrap();
    assert_eq!(head(path).await.unwrap().status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn path_download_forbids_configured_pontia_home() {
    let app = fixture().await;
    let file = app.pontia_home().path().join("private.txt");
    std::fs::write(&file, b"private").unwrap();
    assert_eq!(
        path_content(&app, "s", &file, true).await.status(),
        StatusCode::FORBIDDEN
    );
}
