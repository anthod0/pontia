use super::{common::test_app::TestApp, support::http::json_response};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::Value;
use tower::ServiceExt;
const ID: &str = "12345678-1234-4234-8234-123456789abc";
const CREATE: &str = "/api/v1/uploads?name=hello.txt&size=3&sha256=ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
async fn request(
    app: &TestApp,
    method: &str,
    path: &str,
    body: Vec<u8>,
    content_type: &str,
) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(path)
        .header("Authorization", "Bearer test-token")
        .header("Idempotency-Key", ID)
        .header("Content-Type", content_type)
        .body(Body::from(body))
        .unwrap();
    json_response(
        pontia_http::router(app.state.clone())
            .oneshot(request)
            .await
            .unwrap(),
    )
    .await
}
#[tokio::test]
async fn upload_api_preserves_progress_and_reports_contract_errors() {
    let app = TestApp::new().await;
    let (status, created) = request(&app, "POST", CREATE, vec![], "").await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["data"]["upload"]["offset"], 0);
    let path = format!("/api/v1/uploads/{ID}");
    let (status, error) = request(
        &app,
        "PATCH",
        &format!("{path}?offset=0"),
        b"abc".to_vec(),
        "text/plain",
    )
    .await;
    assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE);
    assert_eq!(error["error"]["code"], "unsupported_media_type");
    let (status, upload) = request(
        &app,
        "PATCH",
        &format!("{path}?offset=0"),
        b"abc".to_vec(),
        "application/octet-stream",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(upload["data"]["upload"]["offset"], 3);
    let (_, retry) = request(&app, "POST", CREATE, vec![], "").await;
    assert_eq!(retry["data"]["upload"]["offset"], 3);
    let (status, error) = request(
        &app,
        "PATCH",
        &format!("{path}?offset=0"),
        b"abc".to_vec(),
        "application/octet-stream",
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(error["error"]["code"], "upload_offset_conflict");
    let (status, completed) = request(&app, "POST", &format!("{path}/complete"), vec![], "").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(completed["data"]["upload"]["completed"], true);
    assert_eq!(
        std::fs::read(completed["data"]["upload"]["path"].as_str().unwrap()).unwrap(),
        b"abc"
    );
    for query in ["-1", "+1", "1.5", "no"] {
        let (status, error) = request(
            &app,
            "PATCH",
            &format!("{path}?offset={query}"),
            b"x".to_vec(),
            "application/octet-stream",
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(error["error"]["code"], "invalid_request");
    }
    let (status, error) = request(&app, "POST", CREATE, b"unexpected".to_vec(), "").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(error["error"]["code"], "invalid_request");
}
