use crate::common::test_app::TestApp;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

#[tokio::test]
async fn shutdown_requires_local_bearer_and_preserves_unverified_processes() {
    let app = TestApp::new().await;
    let router = pontia_http::router(app.state.clone());
    let dir = app.pontia_home().path().join("state/codex");
    std::fs::create_dir_all(&dir).unwrap();
    let socket = dir.join("background-app-server.sock");
    let _listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
    let request = |token, force, stop| {
        Request::post("/api/v1/shutdown")
            .header("authorization", token)
            .header("content-type", "application/json")
            .body(Body::from(
                json!({"pontia_home":app.pontia_home().path(),"operation":"down", "force":force, "stop":stop}).to_string(),
            ))
            .unwrap()
    };

    let denied = router
        .clone()
        .oneshot(request("Bearer wrong", true, true))
        .await
        .unwrap();
    assert_eq!(denied.status(), StatusCode::UNAUTHORIZED);
    for (force, stop) in [(true, false), (false, true), (true, true)] {
        let response = router
            .clone()
            .oneshot(request("Bearer test-token", force, stop))
            .await
            .unwrap();
        let status = response.status();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let replies: Vec<Value> = serde_json::Deserializer::from_slice(&bytes)
            .into_iter()
            .collect::<Result<_, _>>()
            .unwrap();
        if !force {
            assert_eq!(status, StatusCode::CONFLICT);
            assert!(
                replies[0]["error"]["message"]
                    .as_str()
                    .unwrap()
                    .contains(socket.to_str().unwrap())
            );
        } else {
            assert_eq!(status, StatusCode::OK);
            assert!(
                replies[0]["listing"][0]
                    .as_str()
                    .unwrap()
                    .contains(socket.to_str().unwrap())
            );
            if stop {
                assert!(
                    replies[1]["stopped"][0]
                        .as_str()
                        .unwrap()
                        .contains(socket.to_str().unwrap())
                );
            } else {
                assert_eq!(replies.len(), 1);
            }
        }
    }
    std::os::unix::net::UnixStream::connect(socket).unwrap();
}
