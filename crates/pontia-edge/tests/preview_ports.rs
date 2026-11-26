mod support;

use axum::{body::Body, http::Response};
use futures_util::{SinkExt, StreamExt};
use pontia_edge::{config::PortPreviewConfig, network::bind_listener};
use reqwest::header;
use serde::Deserialize;
use serde_json::json;
use support::{EDGE_ID, TestEdge, signed_jwt};
use tokio_tungstenite::{
    connect_async_tls_with_config,
    tungstenite::{Message, client::IntoClientRequest},
};
use uuid::Uuid;

const DASHBOARD: &str = "https://app.pontia.dev";

#[derive(Deserialize)]
struct OpenedPreview {
    preview_origin: String,
    listen_port: u16,
    target_port: u16,
    remapped: bool,
}

fn available_pair() -> (u16, std::net::TcpListener) {
    for first in 20000..60000 {
        let Ok(occupied) = bind_listener(first) else {
            continue;
        };
        let Ok(second) = bind_listener(first + 1) else {
            continue;
        };
        drop(second);
        return (first, occupied);
    }
    panic!("no consecutive preview ports are available");
}

fn free_pair() -> (u16, u16) {
    let first = bind_listener(0).unwrap();
    let second = bind_listener(0).unwrap();
    let mut ports = [
        first.local_addr().unwrap().port(),
        second.local_addr().unwrap().port(),
    ];
    ports.sort_unstable();
    drop((first, second));
    (ports[0], ports[1])
}

fn preview_token(device: Uuid, target_port: u16) -> String {
    let now = jsonwebtoken::get_current_timestamp();
    signed_jwt(
        "pontia-preview+jwt",
        &json!({
            "purpose":"localhost_preview",
            "aud": EDGE_ID,
            "sub": Uuid::now_v7(),
            "did": device,
            "port": target_port,
            "iat": now,
            "exp": now + 14400,
        }),
        &[5; 32],
    )
}

async fn open_preview(server: &TestEdge, token: &str) -> OpenedPreview {
    server
        .http
        .post(format!("{}/__pontia/preview/open", server.edge_origin))
        .header(header::ORIGIN, DASHBOARD)
        .bearer_auth(token)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap()
}

#[tokio::test]
async fn prefers_the_target_port_and_isolates_two_devices_with_the_same_target() {
    let (target_port, fallback_port) = free_pair();
    let server = TestEdge::start_with_preview(Some(PortPreviewConfig {
        ranges: vec![target_port.to_string(), fallback_port.to_string()],
        max_listeners: 2,
    }))
    .await;
    let first = Uuid::now_v7();
    let second = Uuid::now_v7();
    let _first = server
        .device(first, |_| async { Response::new(Body::from("first")) })
        .await;
    let _second = server
        .device(second, |_| async { Response::new(Body::from("second")) })
        .await;
    let first_token = preview_token(first, target_port);
    let second_token = preview_token(second, target_port);

    let first_open = open_preview(&server, &first_token).await;
    let second_open = open_preview(&server, &second_token).await;
    assert_eq!(first_open.listen_port, target_port);
    assert!(!first_open.remapped);
    assert_eq!(second_open.listen_port, fallback_port);
    assert!(second_open.remapped);
    assert_ne!(first_open.preview_origin, second_open.preview_origin);
}

#[tokio::test]
async fn an_open_preview_listener_uses_reloaded_main_tls_configuration() {
    let (target_port, _) = free_pair();
    let server = TestEdge::start_with_preview(Some(PortPreviewConfig {
        ranges: vec![target_port.to_string()],
        max_listeners: 1,
    }))
    .await;
    let device = Uuid::now_v7();
    let _device = server
        .device(device, |_| async { Response::new(Body::from("preview")) })
        .await;
    let opened = open_preview(&server, &preview_token(device, target_port)).await;
    assert_eq!(
        server
            .http
            .get(&opened.preview_origin)
            .send()
            .await
            .unwrap()
            .status(),
        401
    );

    let rcgen::CertifiedKey { cert, signing_key } =
        rcgen::generate_simple_self_signed(vec!["127.0.0.1".into()]).unwrap();
    let pem = server.root.path().join("reloaded.pem");
    std::fs::write(
        &pem,
        format!("{}{}", cert.pem(), signing_key.serialize_pem()),
    )
    .unwrap();
    server
        .edge_tls
        .reload_from_pem_file(&pem, &pem)
        .await
        .unwrap();
    let reloaded_client = reqwest::Client::builder()
        .add_root_certificate(reqwest::Certificate::from_der(cert.der()).unwrap())
        .build()
        .unwrap();
    assert_eq!(
        reloaded_client
            .get(opened.preview_origin)
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
}

#[tokio::test]
async fn opens_a_tls_preview_port_and_falls_back_from_an_occupied_matching_port() {
    let (target_port, occupied) = available_pair();
    let config = PortPreviewConfig {
        ranges: vec![format!("{target_port}-{}", target_port + 1)],
        max_listeners: 2,
    };
    let server = TestEdge::start_with_preview(Some(config)).await;
    let device = Uuid::now_v7();
    let _device = server
        .device(device, move |request| async move {
            assert_eq!(
                request.uri().path(),
                format!("/preview/v1/http/{target_port}/page")
            );
            assert_eq!(
                request.headers()["x-pontia-preview-host"],
                format!("127.0.0.1:{}", target_port + 1)
            );
            assert_eq!(request.headers()[header::COOKIE], "app=one");
            Response::new(Body::from("preview"))
        })
        .await;
    let now = jsonwebtoken::get_current_timestamp();
    let claims = json!({
        "purpose":"localhost_preview",
        "aud": EDGE_ID,
        "sub": Uuid::now_v7(),
        "did": device,
        "port": target_port,
        "iat": now,
        "exp": now + 14400,
    });
    let token = signed_jwt("pontia-preview+jwt", &claims, &[5; 32]);
    let response = server
        .http
        .post(format!("{}/__pontia/preview/open", server.edge_origin))
        .header(header::ORIGIN, DASHBOARD)
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let opened: OpenedPreview = response.json().await.unwrap();
    assert_eq!(opened.listen_port, target_port + 1);
    assert_eq!(opened.target_port, target_port);
    assert!(opened.remapped);
    assert_eq!(
        opened.preview_origin,
        format!("https://127.0.0.1:{}", target_port + 1)
    );
    let reused: OpenedPreview = server
        .http
        .post(format!("{}/__pontia/preview/open", server.edge_origin))
        .header(header::ORIGIN, DASHBOARD)
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(reused.preview_origin, opened.preview_origin);
    drop(occupied);

    let response = server
        .http
        .post(format!("{}/__pontia_auth", opened.preview_origin))
        .header(header::ORIGIN, DASHBOARD)
        .form(&[("token", token.as_str()), ("redirect", "/page")])
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 303);
    let cookie = response.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split_once(';')
        .unwrap()
        .0
        .to_owned();
    assert!(cookie.starts_with(&format!("__Host-pontia_preview_{}=", target_port + 1)));

    let response = server
        .http
        .get(format!("{}/page", opened.preview_origin))
        .header(
            header::COOKIE,
            format!(
                "app=one; __Host-pontia_preview_{}=other; {cookie}",
                target_port + 2
            ),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.text().await.unwrap(), "preview");
}

#[tokio::test]
async fn reports_port_preview_as_unavailable_when_it_is_disabled() {
    let server = TestEdge::start().await;
    let response = server
        .http
        .post(format!("{}/__pontia/preview/open", server.edge_origin))
        .header(header::ORIGIN, DASHBOARD)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 409);
    assert_eq!(
        response.json::<serde_json::Value>().await.unwrap(),
        json!({"error":"preview_unavailable"})
    );
}

#[tokio::test]
async fn open_requires_dashboard_origin_valid_token_and_online_device() {
    let listener = bind_listener(0).unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let server = TestEdge::start_with_preview(Some(PortPreviewConfig {
        ranges: vec![port.to_string()],
        max_listeners: 1,
    }))
    .await;
    let device = Uuid::now_v7();
    let now = jsonwebtoken::get_current_timestamp();
    let token = signed_jwt(
        "pontia-preview+jwt",
        &json!({
            "purpose":"localhost_preview",
            "aud": EDGE_ID,
            "sub": Uuid::now_v7(),
            "did": device,
            "port": 5173,
            "iat": now,
            "exp": now + 14400,
        }),
        &[5; 32],
    );
    let endpoint = format!("{}/__pontia/preview/open", server.edge_origin);

    assert_eq!(
        server
            .http
            .post(&endpoint)
            .bearer_auth(&token)
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    assert_eq!(
        server
            .http
            .post(&endpoint)
            .header(header::ORIGIN, DASHBOARD)
            .bearer_auth("invalid")
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    assert_eq!(
        server
            .http
            .post(&endpoint)
            .header(header::ORIGIN, DASHBOARD)
            .bearer_auth(token)
            .send()
            .await
            .unwrap()
            .status(),
        503
    );
}

#[tokio::test]
async fn capacity_evicts_the_idle_lru_binding_and_rejects_its_old_cookie() {
    let listener = bind_listener(0).unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let server = TestEdge::start_with_preview(Some(PortPreviewConfig {
        ranges: vec![port.to_string()],
        max_listeners: 1,
    }))
    .await;
    let first = Uuid::now_v7();
    let second = Uuid::now_v7();
    let _first_device = server
        .device(first, |request| async {
            if request.uri().path().contains("/ws/") {
                Response::new(request.into_body())
            } else {
                Response::new(Body::from("first"))
            }
        })
        .await;
    let _second_device = server
        .device(second, |_| async { Response::new(Body::from("second")) })
        .await;
    let make_token = |device| {
        let now = jsonwebtoken::get_current_timestamp();
        signed_jwt(
            "pontia-preview+jwt",
            &json!({
                "purpose":"localhost_preview",
                "aud": EDGE_ID,
                "sub": Uuid::now_v7(),
                "did": device,
                "port": 5173,
                "iat": now,
                "exp": now + 14400,
            }),
            &[5; 32],
        )
    };
    let first_token = make_token(first);
    let second_token = make_token(second);
    let endpoint = format!("{}/__pontia/preview/open", server.edge_origin);
    let first_open: OpenedPreview = server
        .http
        .post(&endpoint)
        .header(header::ORIGIN, DASHBOARD)
        .bearer_auth(&first_token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let old_cookie = server
        .http
        .post(format!("{}/__pontia_auth", first_open.preview_origin))
        .header(header::ORIGIN, DASHBOARD)
        .form(&[("token", first_token.as_str())])
        .send()
        .await
        .unwrap()
        .headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split_once(';')
        .unwrap()
        .0
        .to_owned();

    let mut websocket_request = format!(
        "{}/hmr",
        first_open.preview_origin.replacen("https:", "wss:", 1)
    )
    .into_client_request()
    .unwrap();
    websocket_request
        .headers_mut()
        .insert(header::ORIGIN, first_open.preview_origin.parse().unwrap());
    websocket_request
        .headers_mut()
        .insert(header::COOKIE, old_cookie.parse().unwrap());
    let (mut websocket, _) = connect_async_tls_with_config(
        websocket_request,
        None,
        false,
        Some(server.connector.clone()),
    )
    .await
    .unwrap();
    websocket.send(Message::text("active")).await.unwrap();
    assert_eq!(
        websocket.next().await.unwrap().unwrap(),
        Message::text("active")
    );

    assert_eq!(
        server
            .http
            .post(&endpoint)
            .header(header::ORIGIN, DASHBOARD)
            .bearer_auth(&second_token)
            .send()
            .await
            .unwrap()
            .status(),
        503
    );
    websocket.close(None).await.unwrap();
    drop(websocket);
    tokio::task::yield_now().await;

    let second_open: OpenedPreview = server
        .http
        .post(&endpoint)
        .header(header::ORIGIN, DASHBOARD)
        .bearer_auth(second_token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(second_open.listen_port, first_open.listen_port);
    assert_eq!(
        server
            .http
            .get(first_open.preview_origin)
            .header(header::COOKIE, old_cookie)
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
}
