use super::{
    common::test_app::TestApp,
    support::http::{get_json, post_json},
};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

async fn fixture() -> TestApp {
    let app = TestApp::new().await;
    sqlx::query("INSERT INTO sessions (session_id, client_type, state) VALUES ('s', 'generic', 'idle'), ('other', 'generic', 'idle')")
        .execute(&app.db).await.unwrap();
    app
}

async fn share(app: &TestApp, path: &std::path::Path) -> Value {
    let (status, body) = post_json(
        app.state.clone(),
        "/api/v1/sessions/s/files",
        json!({"path": path}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    body["data"]["file"].clone()
}

async fn content(app: &TestApp, session: &str, id: &str) -> axum::response::Response {
    pontia_http::router(app.state.clone())
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/sessions/{session}/files/{id}/content"))
                .header("Authorization", "Bearer test-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
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
async fn path_download_reads_current_file_without_creating_shares_or_events() {
    let app = fixture().await;
    let path = app.workspace().path().join("报告 #1.txt");
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
    for table in ["session_files", "events"] {
        let count: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
            .fetch_one(&app.db)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }
    std::fs::remove_file(&path).unwrap();
    assert_eq!(
        path_content(&app, "s", &path, true).await.status(),
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn path_download_requires_authentication_session_and_absolute_regular_file_within_size_limit()
{
    let app = fixture().await;
    let path = app.workspace().path().join("report.txt");
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
        path_content(&app, "s", app.workspace().path(), true)
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
    let target = app.workspace().path().join("target");
    let path = app.workspace().path().join("link");
    std::fs::write(&target, b"linked file").unwrap();
    std::os::unix::fs::symlink(&target, &path).unwrap();
    let response = path_content(&app, "s", &path, true).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.into_body().collect().await.unwrap().to_bytes(),
        &b"linked file"[..]
    );
    let socket_path = app.workspace().path().join("socket");
    let _socket = std::os::unix::net::UnixListener::bind(&socket_path).unwrap();
    assert_ne!(
        path_content(&app, "s", &socket_path, true).await.status(),
        StatusCode::OK
    );
}

#[tokio::test]
async fn declaration_and_event_are_atomic_and_streams_enrich_replayed_events() {
    let app = fixture().await;
    let path = app.workspace().path().join("report.txt");
    std::fs::write(&path, b"original").unwrap();
    let first = share(&app, &path).await;
    assert!(first["turn_id"].is_null());
    sqlx::query("INSERT INTO turns (turn_id, session_id, state) VALUES ('t', 's', 'running'), ('other-turn', 'other', 'running')").execute(&app.db).await.unwrap();
    let second = share(&app, &path).await;
    assert_eq!(second["turn_id"], "t");
    let payload: String = sqlx::query_scalar("SELECT payload FROM events WHERE turn_id='t'")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&payload).unwrap(),
        json!({"file_id": second["file_id"]})
    );
    for path in [
        "/api/v1/dashboard/events/stream?after=session:0",
        "/api/v1/sessions/s/events/stream",
    ] {
        let response = pontia_http::router(app.state.clone())
            .oneshot(
                Request::builder()
                    .uri(path)
                    .header("Authorization", "Bearer test-token")
                    .header("x-pontia-test-stream-once", "true")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let text = String::from_utf8(bytes.to_vec()).unwrap();
        let events: Vec<Value> = text
            .lines()
            .filter_map(|l| l.strip_prefix("data: "))
            .map(|data| serde_json::from_str(data).unwrap())
            .collect();
        assert_eq!(events.len(), 2);
        let event = if events[1].get("event").is_some() {
            &events[1]["event"]
        } else {
            &events[1]
        };
        assert_eq!(event["file"], second);
    }
    sqlx::query("CREATE TRIGGER reject_file_event BEFORE INSERT ON events BEGIN SELECT RAISE(ABORT, 'test failure'); END").execute(&app.db).await.unwrap();
    assert_eq!(
        post_json(
            app.state.clone(),
            "/api/v1/sessions/s/files",
            json!({"path": path})
        )
        .await
        .0,
        StatusCode::INTERNAL_SERVER_ERROR
    );
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM session_files")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(count, 2);
}

#[tokio::test]
async fn download_reads_current_bytes_and_rejects_retargeted_paths() {
    let app = fixture().await;
    let path = app.workspace().path().join("图片.svg");
    std::fs::write(&path, b"\x89PNG\r\n\x1a\noriginal").unwrap();
    let record = share(&app, &path).await;
    assert_eq!(record["media_type"], "image/png");
    let id = record["file_id"].as_str().unwrap();
    std::fs::write(&path, b"<svg onload='alert(1)'/>").unwrap();
    let response = content(&app, "s", id).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()["content-type"],
        "application/octet-stream"
    );
    assert_eq!(response.headers()["cache-control"], "no-store");
    assert_eq!(response.headers()["x-content-type-options"], "nosniff");
    assert!(
        response.headers()["content-disposition"]
            .to_str()
            .unwrap()
            .starts_with("attachment; filename*=UTF-8''")
    );
    assert_eq!(
        response.into_body().collect().await.unwrap().to_bytes(),
        &b"<svg onload='alert(1)'/>"[..]
    );
    assert_eq!(
        content(&app, "other", id).await.status(),
        StatusCode::NOT_FOUND
    );
    std::fs::remove_file(&path).unwrap();
    assert_eq!(content(&app, "s", id).await.status(), StatusCode::NOT_FOUND);
    let target = app.workspace().path().join("target");
    std::fs::write(&target, b"secret").unwrap();
    std::os::unix::fs::symlink(&target, &path).unwrap();
    assert_eq!(content(&app, "s", id).await.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn checks_size_on_declaration_and_each_download_and_requires_absolute_regular_file() {
    let app = fixture().await;
    for path in [std::path::Path::new("relative"), app.workspace().path()] {
        assert_eq!(
            post_json(
                app.state.clone(),
                "/api/v1/sessions/s/files",
                json!({"path": path})
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
    }
    assert_eq!(
        post_json(
            app.state.clone(),
            "/api/v1/sessions/missing/files",
            json!({"path": app.workspace().path()})
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    let path = app.workspace().path().join("large");
    let file = std::fs::File::create(&path).unwrap();
    file.set_len(1 << 30).unwrap();
    let record = share(&app, &path).await;
    assert_eq!(
        content(&app, "s", record["file_id"].as_str().unwrap())
            .await
            .status(),
        StatusCode::OK
    );
    file.set_len((1 << 30) + 1).unwrap();
    assert_eq!(
        content(&app, "s", record["file_id"].as_str().unwrap())
            .await
            .status(),
        StatusCode::PAYLOAD_TOO_LARGE
    );
    assert_eq!(
        post_json(
            app.state.clone(),
            "/api/v1/sessions/s/files",
            json!({"path": path})
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    let response = pontia_http::router(app.state.clone())
        .oneshot(
            Request::builder()
                .uri("/api/v1/sessions/s/files")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn pagination_uses_time_then_id_without_skipping_equal_timestamps() {
    let app = fixture().await;
    let path = app.workspace().path().join("report");
    std::fs::write(&path, b"a").unwrap();
    let mut expected = Vec::new();
    for _ in 0..3 {
        expected.push(
            share(&app, &path).await["file_id"]
                .as_str()
                .unwrap()
                .to_string(),
        );
    }
    sqlx::query("UPDATE session_files SET created_at='2026-11-05T00:00:00Z'")
        .execute(&app.db)
        .await
        .unwrap();
    expected.sort_by(|a, b| b.cmp(a));
    let (_, page) = get_json(app.state.clone(), "/api/v1/sessions/s/files?limit=2").await;
    let files = page["data"]["files"].as_array().unwrap();
    assert_eq!(
        files
            .iter()
            .map(|f| f["file_id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        expected[..2]
    );
    let cursor = page["data"]["next_cursor"].as_str().unwrap();
    let (_, page) = get_json(
        app.state.clone(),
        &format!("/api/v1/sessions/s/files?limit=2&cursor={cursor}"),
    )
    .await;
    assert_eq!(page["data"]["files"][0]["file_id"], expected[2]);
    assert!(page["data"]["next_cursor"].is_null());
    assert_eq!(
        get_json(
            app.state.clone(),
            "/api/v1/sessions/other/files?cursor=unknown"
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
}
