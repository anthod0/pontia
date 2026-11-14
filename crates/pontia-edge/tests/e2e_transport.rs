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
            assert!(!request.headers().contains_key(header::AUTHORIZATION));
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

    let token = server.admit(device_id).await;
    let response = server
        .http
        .post(format!("{}/e2e/v1/requests", server.edge_origin))
        .header(header::ORIGIN, ORIGIN)
        .header(header::CONTENT_TYPE, "application/pontia-e2e")
        .bearer_auth(&token)
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
    let response = server
        .http
        .post(format!("{}/e2e/v1/sessions", server.edge_origin))
        .header(header::ORIGIN, "https://attacker.example")
        .header(header::CONTENT_TYPE, "application/pontia-e2e")
        .body("ciphertext")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn both_e2e_routes_reject_invalid_tokens_before_forwarding() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    let server = TestEdge::start().await;
    let device_id = Uuid::now_v7();
    let received = Arc::new(AtomicUsize::new(0));
    let counter = received.clone();
    let _device = server
        .device(device_id, move |request: Request<Body>| {
            let counter = counter.clone();
            async move {
                counter.fetch_add(1, Ordering::SeqCst);
                assert!(!request.headers().contains_key(header::AUTHORIZATION));
                Response::new(Body::from("encrypted-response"))
            }
        })
        .await;
    let token = server.admit(device_id).await;
    for route in ["sessions", "requests"] {
        let url = format!("{}/e2e/v1/{route}", server.edge_origin);
        for authorization in [
            None,
            Some("Bearer unknown".to_owned()),
            Some(format!("Basic {token}")),
            Some(format!("Bearer {token} extra")),
        ] {
            let mut request = server
                .http
                .post(&url)
                .header(header::ORIGIN, ORIGIN)
                .header(header::CONTENT_TYPE, "application/pontia-e2e");
            if let Some(value) = authorization {
                request = request.header(header::AUTHORIZATION, value);
            }
            let response = request.body("ciphertext").send().await.unwrap();
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
            assert_eq!(
                response.headers()[header::ACCESS_CONTROL_ALLOW_ORIGIN],
                ORIGIN
            );
            assert_eq!(
                response.json::<serde_json::Value>().await.unwrap(),
                serde_json::json!({"error": "invalid_admission_token"})
            );
        }
        assert_eq!(
            received.load(Ordering::SeqCst),
            if route == "sessions" { 0 } else { 1 }
        );
        let response = server
            .http
            .post(&url)
            .header(header::ORIGIN, ORIGIN)
            .header(header::CONTENT_TYPE, "application/pontia-e2e")
            .bearer_auth(&token)
            .body("ciphertext")
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
    assert_eq!(received.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn admission_tokens_route_both_transports_to_their_bound_devices() {
    let server = TestEdge::start().await;
    let mut devices = Vec::new();
    let mut tokens = Vec::new();
    for marker in ["first-device", "second-device"] {
        let device_id = Uuid::now_v7();
        devices.push(
            server
                .device(device_id, move |request: Request<Body>| async move {
                    assert!(!request.headers().contains_key(header::AUTHORIZATION));
                    Response::new(Body::from(format!("{marker}:{}", request.uri().path())))
                })
                .await,
        );
        tokens.push((server.admit(device_id).await, marker));
    }
    for route in ["sessions", "requests"] {
        let url = format!("{}/e2e/v1/{route}", server.edge_origin);
        for (token, marker) in &tokens {
            let response = server
                .http
                .post(&url)
                .header(header::ORIGIN, ORIGIN)
                .header(header::CONTENT_TYPE, "application/pontia-e2e")
                .bearer_auth(token)
                .body("ciphertext")
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(
                response.text().await.unwrap(),
                format!("{marker}:/e2e/v1/{route}")
            );
        }
        let offline_token = server.admit(Uuid::now_v7()).await;
        let response = server
            .http
            .post(&url)
            .header(header::ORIGIN, ORIGIN)
            .header(header::CONTENT_TYPE, "application/pontia-e2e")
            .bearer_auth(offline_token)
            .body("ciphertext")
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }
}

#[tokio::test]
async fn admission_exchange_enforces_one_time_tickets_and_service_error_contract() {
    let server = TestEdge::start().await;
    let ticket = server.issue_admission_ticket(Uuid::now_v7());
    let url = format!("{}/browser-admissions", server.edge_origin);
    let response = server
        .http
        .post(&url)
        .header(header::ORIGIN, ORIGIN)
        .json(&serde_json::json!({"ticket": ticket}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    assert_eq!(
        response.headers()[header::ACCESS_CONTROL_ALLOW_ORIGIN],
        ORIGIN
    );
    assert_eq!(
        response.json::<serde_json::Value>().await.unwrap()["token"]
            .as_str()
            .unwrap()
            .len(),
        43
    );
    for (ticket, status, error) in [
        (ticket, StatusCode::UNAUTHORIZED, "invalid_admission_ticket"),
        (
            server.issue_ticket(Uuid::now_v7()),
            StatusCode::UNAUTHORIZED,
            "invalid_admission_ticket",
        ),
        (
            "invalid-credentials".into(),
            StatusCode::SERVICE_UNAVAILABLE,
            "service_unavailable",
        ),
        (
            "uncertain".into(),
            StatusCode::SERVICE_UNAVAILABLE,
            "service_unavailable",
        ),
    ] {
        let response = server
            .http
            .post(&url)
            .header(header::ORIGIN, ORIGIN)
            .json(&serde_json::json!({"ticket": ticket}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), status);
        assert_eq!(
            response.headers()[header::ACCESS_CONTROL_ALLOW_ORIGIN],
            ORIGIN
        );
        assert_eq!(
            response.json::<serde_json::Value>().await.unwrap(),
            serde_json::json!({"error": error})
        );
    }
    server.stop_cloud();
    let response = server
        .http
        .post(&url)
        .header(header::ORIGIN, ORIGIN)
        .json(&serde_json::json!({"ticket": "unused"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn browser_routes_support_authenticated_cors_and_strict_admission_json() {
    let server = TestEdge::start().await;
    for path in [
        "/browser-admissions".to_owned(),
        "/e2e/v1/sessions".to_owned(),
        "/e2e/v1/requests".to_owned(),
    ] {
        let response = server
            .http
            .request(
                reqwest::Method::OPTIONS,
                format!("{}{path}", server.edge_origin),
            )
            .header(header::ORIGIN, ORIGIN)
            .header(header::ACCESS_CONTROL_REQUEST_METHOD, "POST")
            .header(
                header::ACCESS_CONTROL_REQUEST_HEADERS,
                "content-type, authorization",
            )
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert_eq!(
            response.headers()[header::ACCESS_CONTROL_ALLOW_HEADERS],
            "content-type, authorization"
        );
    }
    let url = format!("{}/browser-admissions", server.edge_origin);
    for (content_type, body) in [
        ("text/plain", "{}"),
        ("application/json", "{"),
        ("application/json", "{\"ticket\":3}"),
        (
            "application/json",
            "{\"ticket\":\"ticket\",\"device_id\":\"override\"}",
        ),
    ] {
        let response = server
            .http
            .post(&url)
            .header(header::ORIGIN, ORIGIN)
            .header(header::CONTENT_TYPE, content_type)
            .body(body)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            response.headers()[header::ACCESS_CONTROL_ALLOW_ORIGIN],
            ORIGIN
        );
        assert_eq!(
            response.json::<serde_json::Value>().await.unwrap(),
            serde_json::json!({"error": "invalid_request"})
        );
    }
    let response = server
        .http
        .post(&url)
        .header(header::ORIGIN, "https://attacker.example")
        .json(&serde_json::json!({"ticket": server.issue_admission_ticket(Uuid::now_v7())}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert!(
        !response
            .headers()
            .contains_key(header::ACCESS_CONTROL_ALLOW_ORIGIN)
    );
}
