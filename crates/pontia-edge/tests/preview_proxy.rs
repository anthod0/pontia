mod support;

use axum::{
    body::{Body, Bytes},
    http::{Response, StatusCode, header},
};
use futures_util::{SinkExt, StreamExt, stream};
use reqwest::{Method, RequestBuilder};
use serde_json::{Value, json};
use support::{EDGE_ID, TestEdge, signed_jwt};
use tokio_tungstenite::{
    connect_async_tls_with_config,
    tungstenite::{Message, client::IntoClientRequest},
};
use uuid::Uuid;

const DASHBOARD: &str = "https://app.pontia.dev";
const TYP: &str = "pontia-preview+jwt";

struct Preview {
    server: TestEdge,
    device: Uuid,
    host: String,
    claims: Value,
}

impl Preview {
    async fn start() -> Self {
        let server = TestEdge::start().await;
        let device = Uuid::now_v7();
        let now = jsonwebtoken::get_current_timestamp();
        let claims = json!({"purpose":"localhost_preview","aud":EDGE_ID,"sub":Uuid::now_v7(),"did":device,"port":5173,"iat":now,"exp":now+14400});
        let host = format!("5173-{}.test.p.pontia.dev", device.simple());
        Self {
            server,
            device,
            host,
            claims,
        }
    }

    fn request(&self, method: Method, path: &str) -> RequestBuilder {
        self.server
            .http
            .request(method, format!("{}{path}", self.server.edge_origin))
            .header(header::HOST, &self.host)
    }

    fn exchange(&self, token: &str, redirect: &str) -> RequestBuilder {
        self.request(Method::POST, "/__pontia_auth")
            .header(header::ORIGIN, DASHBOARD)
            .form(&[("token", token), ("redirect", redirect)])
    }

    async fn cookie(&self) -> String {
        let token = signed_jwt(TYP, &self.claims, &[5; 32]);
        let res = self
            .exchange(&token, "/app?q=1#section")
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), 303);
        assert_eq!(res.headers()[header::LOCATION], "/app?q=1#section");
        assert_eq!(res.headers()[header::CACHE_CONTROL], "no-store");
        assert_eq!(res.headers()[header::REFERRER_POLICY], "no-referrer");
        assert_eq!(res.headers()[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
        let cookie = res.headers()[header::SET_COOKIE].to_str().unwrap();
        let (cookie, attrs) = cookie.split_once("; ").unwrap();
        assert_eq!(cookie, format!("__Host-pontia_preview={token}"));
        let age = attrs
            .strip_prefix("Path=/; HttpOnly; Secure; SameSite=Lax; Max-Age=")
            .unwrap()
            .parse::<u64>()
            .unwrap();
        assert!((1..=14400).contains(&age));
        cookie.into()
    }
}

#[tokio::test]
async fn isolates_hosts_and_reserves_the_probe() {
    let p = Preview::start().await;
    let did = p.device.simple();
    for label in [
        format!("0-{did}"),
        format!("00080-{did}"),
        format!("65536-{did}"),
        format!("80-{}", Uuid::new_v4().simple()),
        format!("nested.80-{did}"),
        "__probe".into(),
    ] {
        let r = p
            .server
            .http
            .get(format!("{}/healthz", p.server.edge_origin))
            .header(header::HOST, format!("{label}.test.p.pontia.dev"))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 404, "{label}");
    }
    let response = p
        .server
        .http
        .get(format!("{}/__pontia_preview_health", p.server.edge_origin))
        .header(header::HOST, "__probe.test.p.pontia.dev")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let unknown = p
        .server
        .http
        .get(format!("{}/tunnel", p.server.edge_origin))
        .header(header::HOST, "wrong.p.pontia.dev")
        .send()
        .await
        .unwrap();
    assert_eq!(unknown.status(), 404);
}

#[tokio::test]
async fn exchange_rejects_invalid_forms_origins_and_redirects() {
    let p = Preview::start().await;
    let token = signed_jwt(TYP, &p.claims, &[5; 32]);
    for redirect in [
        "http://other.test",
        "//other.test",
        "/\\other.test",
        "/a\r\nb",
        "/__pontia_auth?again",
        "/__pontia_auth/path",
    ] {
        let r = p.exchange(&token, redirect).send().await.unwrap();
        assert_eq!(r.status(), 400);
        assert!(!r.headers().contains_key(header::LOCATION));
        assert!(!r.headers().contains_key(header::SET_COOKIE));
        assert_eq!(r.headers()[header::CACHE_CONTROL], "no-store");
    }
    for (request, status) in [
        (p.request(Method::GET, "/__pontia_auth"), 405),
        (
            p.request(Method::POST, "/__pontia_auth")
                .header(header::ORIGIN, "https://other.test"),
            403,
        ),
        (
            p.request(Method::POST, "/__pontia_auth")
                .header(header::ORIGIN, DASHBOARD)
                .body("token=x"),
            400,
        ),
        (
            p.exchange(&token, "/").header(header::ORIGIN, DASHBOARD),
            403,
        ),
        (p.exchange(&token, &"x".repeat(17000)), 413),
    ] {
        assert_eq!(request.send().await.unwrap().status(), status);
    }
    for extra in ["&extra=1", "&token=other", "&redirect=/&redirect=/"] {
        let r = p
            .request(Method::POST, "/__pontia_auth")
            .header(header::ORIGIN, DASHBOARD)
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(format!("token={token}{extra}"))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 400);
    }
}

#[tokio::test]
async fn binds_jwts_to_purpose_device_port_edge_and_lifetime() {
    let p = Preview::start().await;
    let now = jsonwebtoken::get_current_timestamp();
    let mut rejected = vec![
        p.server.token(p.device),
        signed_jwt("JWT", &p.claims, &[5; 32]),
        signed_jwt(TYP, &p.claims, &[7; 32]),
    ];
    for (key, val) in [
        ("purpose", json!("admission")),
        ("aud", json!(Uuid::now_v7())),
        ("did", json!(Uuid::now_v7())),
        ("port", json!(3000)),
        ("sub", json!(Uuid::nil())),
        ("exp", json!(now - 1)),
        ("exp", json!(now + 15000)),
        ("iat", json!(now + 100)),
    ] {
        let mut claims = p.claims.clone();
        claims[key] = val;
        rejected.push(signed_jwt(TYP, &claims, &[5; 32]));
    }
    for token in rejected {
        assert_eq!(p.exchange(&token, "/").send().await.unwrap().status(), 401);
        assert_eq!(
            p.request(Method::GET, "/app")
                .header(header::COOKIE, format!("__Host-pontia_preview={token}"))
                .send()
                .await
                .unwrap()
                .status(),
            401
        );
    }
    let mut mixed = p.claims.clone();
    mixed["device_id"] = json!(p.device);
    mixed["user_id"] = json!(Uuid::now_v7());
    for claims in [&p.claims, &mixed] {
        let token = signed_jwt(TYP, claims, &[5; 32]);
        let r = p
            .server
            .http
            .post(format!("{}/e2e/v1/requests", p.server.edge_origin))
            .header(header::ORIGIN, DASHBOARD)
            .header(header::CONTENT_TYPE, "application/pontia-e2e")
            .bearer_auth(token)
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 401);
    }
}

#[tokio::test]
async fn proxies_control_named_paths_streams_and_app_cookies() {
    let p = Preview::start().await;
    let host = p.host.clone();
    let _device = p
        .server
        .device(p.device, move |req| {
            let host = host.clone();
            async move {
                assert_eq!(req.headers()["x-pontia-preview-host"], host);
                assert_eq!(req.headers()[header::COOKIE], "app=one; theme=dark");
                if req.uri().path() == "/preview/v1/http/5173/events" {
                    let events = stream::iter([Ok::<_, std::convert::Infallible>(
                        Bytes::from_static(b"data: first\n\n"),
                    )])
                    .chain(stream::pending());
                    return Response::builder()
                        .header(header::CONTENT_TYPE, "text/event-stream")
                        .body(Body::from_stream(events))
                        .unwrap();
                }
                if req.method() == Method::GET {
                    return Response::new(Body::from(req.uri().path().to_owned()));
                }
                assert_eq!(req.method(), Method::POST);
                assert_eq!(req.uri().query(), Some("q=1"));
                assert_eq!(req.uri().path(), "/preview/v1/http/5173/e2e/v1/requests");
                Response::builder()
                    .header(header::SET_COOKIE, "a=1; Domain = .pontia.dev; Secure")
                    .header(header::SET_COOKIE, "__HoSt-pontia_preview=bad; Path=/")
                    .header(header::SET_COOKIE, "b=2")
                    .body(req.into_body())
                    .unwrap()
            }
        })
        .await;
    let cookie = p.cookie().await;
    let cookies = format!("app=one; {cookie}; theme=dark");
    for path in ["/healthz", "/tunnel"] {
        let r = p
            .request(Method::GET, path)
            .header(header::COOKIE, &cookies)
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 200);
        assert_eq!(
            r.text().await.unwrap(),
            format!("/preview/v1/http/5173{path}")
        );
    }
    let body = vec![9; 2 * 1024 * 1024 + 1];
    let r = p
        .request(Method::POST, "/e2e/v1/requests?q=1")
        .header(header::COOKIE, &cookies)
        .header("x-pontia-preview-host", "spoof.test")
        .body(body.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    assert_eq!(
        r.headers()
            .get_all(header::SET_COOKIE)
            .iter()
            .collect::<Vec<_>>(),
        ["a=1; Secure", "b=2"]
    );
    assert_eq!(r.bytes().await.unwrap(), body);
    let mut r = p
        .request(Method::GET, "/events")
        .header(header::COOKIE, cookies)
        .send()
        .await
        .unwrap();
    assert_eq!(r.headers()[header::CONTENT_TYPE], "text/event-stream");
    assert_eq!(r.chunk().await.unwrap().unwrap(), "data: first\n\n");
    let r = p
        .request(Method::GET, "/healthz")
        .header(header::COOKIE, &cookie)
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 401);
}

#[tokio::test]
async fn returns_navigation_errors_and_preserves_application_errors() {
    let p = Preview::start().await;
    let cookie = p.cookie().await;
    for (cookies, status) in [("", 401), (&cookie[..], 503)] {
        let r = p
            .request(Method::GET, "/")
            .header(header::COOKIE, cookies)
            .header(header::ACCEPT, "text/html")
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), status);
        assert!(
            r.headers()[header::CONTENT_TYPE]
                .to_str()
                .unwrap()
                .starts_with("text/html")
        );
        assert_eq!(r.headers()[header::CACHE_CONTROL], "no-store");
    }
    let _device = p.server.device(p.device, |req| async move {
        let old_error = r#"{"error":{"code":"invalid_tunnel_request","message":"invalid tunneled HTTP request"}}"#;
        match req.uri().path().rsplit('/').next().unwrap() {
            "old" => Response::builder().status(400).body(Body::from(old_error)).unwrap(),
            "app" => Response::builder().status(400).body(Body::from("local error")).unwrap(),
            _ => Response::builder().status(502)
                .header("x-pontia-preview-error", "connection_refused")
                .body(Body::from("refused")).unwrap(),
        }
    }).await;
    for (path, status) in [("/old", 400), ("/unavailable", 502)] {
        let r = p
            .request(Method::GET, path)
            .header(header::COOKIE, &cookie)
            .header("sec-fetch-mode", "navigate")
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), status);
        assert!(
            r.headers()[header::CONTENT_TYPE]
                .to_str()
                .unwrap()
                .starts_with("text/html")
        );
    }
    let r = p
        .request(Method::GET, "/app")
        .header(header::COOKIE, &cookie)
        .header(header::ACCEPT, "text/html")
        .send()
        .await
        .unwrap();
    assert_eq!(r.text().await.unwrap(), "local error");
    let r = p
        .request(Method::GET, "/unavailable")
        .header(header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    assert!(!r.headers().contains_key("x-pontia-preview-error"));
    assert_eq!(r.text().await.unwrap(), "refused");
}

#[tokio::test]
async fn websocket_waits_for_device_and_relays_negotiated_messages() {
    let p = Preview::start().await;
    let cookie = p.cookie().await;
    let _device = p
        .server
        .device(p.device, |req| async move {
            assert_eq!(req.method(), Method::POST);
            assert_eq!(req.headers()[header::ORIGIN], "https://example.test");
            assert!(!req.headers().contains_key(header::COOKIE));
            if req.uri().path().ends_with("/deny") {
                return Response::builder().status(403).body(Body::empty()).unwrap();
            }
            if req.uri().path().ends_with("/oversized") {
                let oversized_header = Bytes::from_static(&[2, 1, 0, 0, 1]);
                return Response::builder()
                    .header(header::SEC_WEBSOCKET_PROTOCOL, "vite-hmr")
                    .body(Body::from(oversized_header))
                    .unwrap();
            }
            assert_eq!(req.uri().path(), "/preview/v1/ws/5173/hmr");
            assert_eq!(req.headers()[header::SEC_WEBSOCKET_PROTOCOL], "vite-hmr");
            Response::builder()
                .header(header::SEC_WEBSOCKET_PROTOCOL, "vite-hmr")
                .body(req.into_body())
                .unwrap()
        })
        .await;
    for path in ["/deny", "/oversized", "/hmr"] {
        let mut req = format!("{}{path}", p.server.edge_origin.replace("https:", "wss:"))
            .into_client_request()
            .unwrap();
        for (name, value) in [
            (header::HOST, p.host.as_str()),
            (header::COOKIE, cookie.as_str()),
            (header::ORIGIN, "https://example.test"),
            (header::SEC_WEBSOCKET_PROTOCOL, "vite-hmr"),
        ] {
            req.headers_mut().insert(name, value.parse().unwrap());
        }
        let result =
            connect_async_tls_with_config(req, None, false, Some(p.server.connector.clone())).await;
        if path == "/deny" {
            assert!(
                matches!(result,Err(tokio_tungstenite::tungstenite::Error::Http(r)) if r.status()==StatusCode::FORBIDDEN)
            );
            continue;
        }
        let (mut ws, r) = result.unwrap();
        assert_eq!(r.headers()[header::SEC_WEBSOCKET_PROTOCOL], "vite-hmr");
        if path == "/oversized" {
            let close = tokio::time::timeout(std::time::Duration::from_secs(2), ws.next())
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            assert!(matches!(close, Message::Close(Some(frame)) if u16::from(frame.code) == 1009));
            continue;
        }
        for msg in [
            Message::text("reload"),
            Message::Binary(Bytes::from_static(b"binary")),
        ] {
            ws.send(msg.clone()).await.unwrap();
            let echo = tokio::time::timeout(std::time::Duration::from_secs(2), ws.next())
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            assert_eq!(echo, msg);
        }
    }
}
