use crate::common::test_app::TestApp;
use axum::{
    body::Body,
    http::{HeaderMap, Method, Request, StatusCode, header},
};
use ed25519_dalek::{Signer, SigningKey};
use http_body_util::BodyExt;
use pontia_e2e::{
    BrowserIdentity, Capability, DeviceIdentity, DeviceSessions,
    bhttp::{Decoder, Encoder, Event, Head, Kind},
};
use pontia_http::{
    HttpEntrypoints,
    e2e::{CONTENT_TYPE, REQUESTS_PATH, SESSIONS_PATH},
};
use tower::ServiceExt;

#[tokio::test]
async fn verified_e2e_and_local_token_access_the_same_business_router() {
    let state = TestApp::builder()
        .external_api_token(Some("local-secret".into()))
        .build_state()
        .await;
    let endpoints = HttpEntrypoints::new(state);
    let identity = DeviceIdentity::from_private_bytes([7; 16], 1, &[11; 32]).unwrap();
    let public = identity.public_key();
    let signer = SigningKey::from_bytes(&[9; 32]);
    let e2e = endpoints.e2e_tunnel(DeviceSessions::new(identity, signer.verifying_key()));
    let browser = BrowserIdentity::generate().unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let mut capability = vec![1];
    capability.extend_from_slice(&[7; 16]);
    capability.extend_from_slice(&1u64.to_be_bytes());
    capability.extend_from_slice(&browser.public_key());
    capability.extend_from_slice(&[13; 32]);
    capability.extend_from_slice(&now.to_be_bytes());
    capability.extend_from_slice(&(now + 60).to_be_bytes());
    capability.extend_from_slice(&[0; 64]);
    let signed = signer.sign(&Capability::decode(&capability).unwrap().signing_bytes());
    capability[105..].copy_from_slice(&signed.to_bytes());
    let handshake = browser.start(&public, &capability).unwrap();
    let outer = |path: &str, wire: Vec<u8>| {
        Request::builder()
            .method("POST")
            .uri(path)
            .header(header::CONTENT_TYPE, CONTENT_TYPE)
            .body(Body::from(wire))
            .unwrap()
    };
    let confirmation = e2e
        .handle(outer(SESSIONS_PATH, handshake.request_bytes().to_vec()))
        .await;
    assert_eq!(confirmation.status(), StatusCode::OK);
    let confirmation = confirmation.into_body().collect().await.unwrap().to_bytes();
    let session = handshake.confirm(&confirmation).unwrap();
    let (context, mut upload, mut download) = session.request().unwrap();
    let (mut bhttp, head) = Encoder::new(&Head::Request {
        method: Method::GET,
        uri: "/api/v1/sessions/overview?sections=list".parse().unwrap(),
        headers: HeaderMap::new(),
    })
    .unwrap();
    let mut wire = vec![1];
    wire.extend_from_slice(&context.session_id);
    wire.extend_from_slice(&context.request_id);
    wire.extend_from_slice(&upload.seal(&head).unwrap());
    wire.extend_from_slice(&upload.seal(&bhttp.finish().unwrap()).unwrap());
    wire.extend_from_slice(&upload.finish().unwrap());
    let response = e2e.handle(outer(REQUESTS_PATH, wire)).await;
    assert_eq!(response.status(), StatusCode::OK);
    let encrypted = response.into_body().collect().await.unwrap().to_bytes();
    let mut remaining = encrypted.as_ref();
    let mut bhttp = Decoder::new(Kind::Response);
    let mut status = None;
    let mut content = Vec::new();
    while !remaining.is_empty() {
        let (used, plaintext) = download.feed(remaining).unwrap();
        remaining = &remaining[used..];
        if let Some(plaintext) = plaintext {
            let mut remaining = plaintext.as_slice();
            while !remaining.is_empty() {
                let (used, event) = bhttp.feed(remaining).unwrap();
                remaining = &remaining[used..];
                match event {
                    Some(Event::Head(Head::Response { status: code, .. })) => status = Some(code),
                    Some(Event::Content(bytes)) => content.extend_from_slice(&bytes),
                    _ => {}
                }
            }
        }
    }
    download.finish().unwrap();
    bhttp.finish().unwrap();
    assert_eq!(status, Some(200));
    let local = endpoints
        .local_http()
        .oneshot(
            Request::builder()
                .uri("/api/v1/sessions/overview?sections=list")
                .header(header::AUTHORIZATION, "Bearer local-secret")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(local.status(), StatusCode::OK);
    let local = local.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&content).unwrap(),
        serde_json::from_slice::<serde_json::Value>(&local).unwrap()
    );
    let spoofed = endpoints
        .local_http()
        .oneshot(
            Request::builder()
                .uri("/api/v1/sessions/overview?sections=list")
                .header("x-pontia-e2e-authenticated", "true")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(spoofed.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        e2e.handle(outer("/api/v1/sessions", vec![])).await.status(),
        StatusCode::NOT_FOUND
    );
}
