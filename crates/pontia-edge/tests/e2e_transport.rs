mod support;

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
    response::Response,
};
use http_body_util::BodyExt;
use support::TestEdge;
use uuid::Uuid;

const ORIGIN: &str = "https://app.pontia.dev";

#[tokio::test]
async fn edge_relays_only_fixed_ciphertext_transports() {
    let server = TestEdge::start().await;
    let device_id = Uuid::now_v7();
    let _device = server
        .device(device_id, |request: Request<Body>| async move {
            assert_eq!(request.method(), "POST");
            assert_eq!(request.uri().path(), "/e2e/v1/requests");
            assert_eq!(
                request.headers().get(header::CONTENT_TYPE).unwrap(),
                "application/pontia-e2e"
            );
            let body = request.into_body().collect().await.unwrap().to_bytes();
            assert_eq!(body.as_ref(), b"ciphertext");
            Response::builder()
                .header(header::CONTENT_TYPE, "application/pontia-e2e")
                .body(Body::from("encrypted-response"))
                .unwrap()
        })
        .await;

    let response = server
        .http
        .post(format!(
            "{}/devices/{device_id}/e2e/v1/requests",
            server.edge_origin
        ))
        .header(header::ORIGIN, ORIGIN)
        .header(header::CONTENT_TYPE, "application/pontia-e2e")
        .body("ciphertext")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.bytes().await.unwrap().as_ref(),
        b"encrypted-response"
    );

    let plaintext = server
        .http
        .get(format!(
            "{}/devices/{device_id}/api/v1/sessions",
            server.edge_origin
        ))
        .header(header::ORIGIN, ORIGIN)
        .send()
        .await
        .unwrap();
    assert_eq!(plaintext.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn edge_rejects_cross_origin_ciphertext_before_device_forwarding() {
    let server = TestEdge::start().await;
    let device_id = Uuid::now_v7();
    let response = server
        .http
        .post(format!(
            "{}/devices/{device_id}/e2e/v1/sessions",
            server.edge_origin
        ))
        .header(header::ORIGIN, "https://attacker.example")
        .header(header::CONTENT_TYPE, "application/pontia-e2e")
        .body("ciphertext")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
