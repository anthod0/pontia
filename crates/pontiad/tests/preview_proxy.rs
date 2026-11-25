use std::{
    convert::Infallible,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    time::Duration,
};

use axum::{
    Router,
    body::{Body, to_bytes},
    extract::{WebSocketUpgrade, ws::Message},
    http::{HeaderMap, Method, Request, StatusCode, request::Builder},
    response::IntoResponse,
    routing::get,
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use bytes::{Bytes, BytesMut};
use ed25519_dalek::{Signer, SigningKey};
use futures_util::{StreamExt, stream};
use pontia_config::PreviewConfig;
use pontia_tunnel::{
    DeviceRequestHandler, TunnelConnection, WsTunnelFrame, connect_edge, serve_device,
};
use pontiad::LocalPreviewProxy;
use serde_json::{Value, json};
use tokio::{
    net::TcpListener,
    sync::{mpsc, watch},
    task::JoinSet,
    time::timeout,
};
use tokio_stream::wrappers::ReceiverStream;
use uuid::Uuid;

const DEVICE: Uuid = Uuid::from_u128(0x0195e7c11b227c339d44123456789abc);
const CLOUD_SEED: [u8; 32] = [5; 32];
const HOST: &str = "5173-0195e7c11b227c339d44123456789abc.test.pontia.run:8443";
const V4: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);
const V6: IpAddr = IpAddr::V6(Ipv6Addr::LOCALHOST);

struct Tunnel {
    connection: TunnelConnection,
    tasks: JoinSet<()>,
    shutdown: watch::Sender<bool>,
}

impl Tunnel {
    async fn start(bound_port: u16) -> Self {
        Self::with_config(bound_port, PreviewConfig::default()).await
    }

    async fn with_config(bound_port: u16, config: PreviewConfig) -> Self {
        let mut tasks = JoinSet::new();
        let (tx, mut rx) = mpsc::channel(1);
        let app = Router::new().route(
            "/tunnel",
            get(move |ws: WebSocketUpgrade| async move {
                ws.on_upgrade(move |socket| async move {
                    let (conn, runtime) = connect_edge(socket).await.unwrap();
                    tx.send(conn).await.unwrap();
                    let _ = runtime.run().await;
                })
            }),
        );
        let edge = TcpListener::bind((V4, 0)).await.unwrap();
        let addr = edge.local_addr().unwrap();
        tasks.spawn(async move { axum::serve(edge, app).await.unwrap() });
        let (socket, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/tunnel"))
            .await
            .unwrap();
        let (shutdown, stop) = watch::channel(false);
        let proxy = LocalPreviewProxy::new(
            bound_port,
            config,
            DEVICE,
            SigningKey::from_bytes(&CLOUD_SEED).verifying_key(),
            stop.clone(),
        );
        let handler = DeviceRequestHandler::new(move |req| {
            let proxy = proxy.clone();
            async move {
                proxy
                    .dispatch(req, async |_| StatusCode::ACCEPTED.into_response())
                    .await
            }
        });
        tasks.spawn(async move {
            let _ = serve_device(socket, handler, stop).await;
        });
        Self {
            connection: rx.recv().await.unwrap(),
            tasks,
            shutdown,
        }
    }
    async fn upstream(&mut self, ip: IpAddr, app: Router) -> u16 {
        let listener = TcpListener::bind((ip, 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        self.tasks
            .spawn(async move { axum::serve(listener, app).await.unwrap() });
        port
    }
}

fn claims(port: u16) -> Value {
    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    json!({"aud": Uuid::now_v7(), "sub": Uuid::now_v7(), "purpose": "localhost_preview",
        "did": DEVICE, "port": port, "iat": now, "exp": now + 14400})
}

fn sign(claims: &Value, seed: &[u8; 32]) -> String {
    let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"EdDSA","typ":"pontia-preview+jwt"}"#);
    let payload = URL_SAFE_NO_PAD.encode(serde_json::to_vec(claims).unwrap());
    let input = format!("{header}.{payload}");
    let signature = SigningKey::from_bytes(seed).sign(input.as_bytes());
    format!("{input}.{}", URL_SAFE_NO_PAD.encode(signature.to_bytes()))
}

fn request(method: Method, kind: &str, port: u16, path: &str) -> Builder {
    Request::builder()
        .method(method)
        .uri(format!(
            "https://pontia-device/preview/v1/{kind}/{port}{path}"
        ))
        .header("x-pontia-preview-host", HOST)
        .header("x-pontia-preview-token", sign(&claims(port), &CLOUD_SEED))
}

#[tokio::test]
async fn rejects_unauthorized_preview_requests_before_connecting_to_loopback() {
    let listener = TcpListener::bind((V4, 0)).await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let tunnel = Tunnel::start(0).await;
    let valid = claims(port);
    let mut invalid = vec![None, Some(sign(&valid, &[8; 32]))];
    for (field, value) in [
        ("did", json!(Uuid::now_v7())),
        ("port", json!(port - 1)),
        ("purpose", json!("device_access")),
        ("exp", valid["iat"].clone()),
    ] {
        let mut altered = valid.clone();
        altered[field] = value;
        invalid.push(Some(sign(&altered, &CLOUD_SEED)));
    }
    for token in invalid {
        for kind in ["http", "ws"] {
            let mut req = request(Method::POST, kind, port, "/")
                .body(Body::empty())
                .unwrap();
            req.headers_mut().remove("x-pontia-preview-token");
            if let Some(value) = &token {
                req.headers_mut()
                    .insert("x-pontia-preview-token", value.parse().unwrap());
            }
            let check = async {
                if kind == "http" {
                    tunnel.connection.request_preview_http(req).await
                } else {
                    tunnel.connection.open_preview_ws(req).await
                }
            };
            tokio::select! {
                reply = check => assert_eq!(reply.unwrap().status(), StatusCode::UNAUTHORIZED),
                _ = listener.accept() => panic!("unauthorized preview reached loopback"),
            }
        }
    }
}

#[tokio::test]
async fn forwards_methods_query_and_headers_on_both_loopback_families() {
    let mut tunnel = Tunnel::start(0).await;
    for ip in [V4, V6] {
        let app = Router::new().fallback(|req: Request<Body>| async move {
            let h = req.headers();
            assert!(!h.contains_key("x-pontia-preview-token"));
            let local = format!("http://{}", h["host"].to_str().unwrap());
            assert_eq!(h["origin"], local);
            assert_eq!(h["referer"], format!("{local}/page?q=1"));
            assert_eq!(h["x-forwarded-host"], HOST);
            assert_eq!(h["x-forwarded-proto"], "https");
            assert!(!h.contains_key("x-pontia-preview-host"));
            let summary = format!(
                "{} {} {}",
                h["host"].to_str().unwrap(),
                req.method(),
                req.uri()
            );
            (
                [
                    ("connection", "x-local"),
                    ("x-local", "removed"),
                    ("x-pontia-preview-error", "spoof"),
                ],
                summary,
            )
        });
        let port = tunnel.upstream(ip, app).await;
        for method in [Method::GET, Method::POST, Method::PUT, Method::DELETE] {
            let req = request(method.clone(), "http", port, "/path%2Fpart?q=%2B")
                .header("origin", format!("https://{HOST}"))
                .header("referer", format!("https://{HOST}/page?q=1"))
                .header("x-forwarded-proto", "spoof")
                .body(Body::empty())
                .unwrap();
            let res = tunnel.connection.request_preview_http(req).await.unwrap();
            assert!(!res.headers().contains_key("x-local"));
            assert!(!res.headers().contains_key("x-pontia-preview-error"));
            assert_eq!(
                to_bytes(res.into_body(), 1024).await.unwrap(),
                format!("localhost:{port} {method} /path%2Fpart?q=%2B")
            );
        }
    }
}

#[tokio::test]
async fn streams_upload_download_and_sse_before_producer_finishes() {
    let mut tunnel = Tunnel::start(0).await;
    let app = Router::new().fallback(|req: Request<Body>| async move {
        let mime = if req.uri().path() == "/events" {
            "text/event-stream"
        } else {
            "application/octet-stream"
        };
        ([("content-type", mime)], req.into_body())
    });
    let port = tunnel.upstream(V4, app).await;
    for (path, chunk) in [
        ("/events", Bytes::from_static(b"data: event\n\n")),
        ("/large", Bytes::from(vec![0xab; 64 * 1024])),
    ] {
        let (tx, rx) = mpsc::channel(1);
        tx.send(Ok::<_, Infallible>(chunk.clone())).await.unwrap();
        let req = request(Method::POST, "http", port, path)
            .body(Body::from_stream(ReceiverStream::new(rx)))
            .unwrap();
        let res = timeout(
            Duration::from_secs(5),
            tunnel.connection.request_preview_http(req),
        )
        .await
        .unwrap()
        .unwrap();
        let mut body = res.into_body().into_data_stream();
        let first = timeout(Duration::from_secs(5), body.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(!first.is_empty());
        let count = if path == "/large" { 81 } else { 2 };
        let expected = chunk.clone();
        let producer = tokio::spawn(async move {
            for _ in 1..count {
                tx.send(Ok(chunk.clone())).await.unwrap();
            }
        });
        let mut output = first.to_vec();
        while let Some(bytes) = body.next().await {
            output.extend(bytes.unwrap());
        }
        producer.await.unwrap();
        assert_eq!(output, expected.repeat(count));
    }
}

#[tokio::test]
async fn preserves_gzip_bytes_and_returns_rewritten_redirect_without_following() {
    let mut tunnel = Tunnel::start(0).await;
    // Empty gzip member.
    const GZIP: &[u8] = &[
        31, 139, 8, 0, 0, 0, 0, 0, 0, 3, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    ];
    let app = Router::new().fallback(|req: Request<Body>| async move {
        if req.uri().path() == "/gzip" {
            return ([("content-encoding", "gzip")], GZIP).into_response();
        }
        let dest = if req.uri().path() == "/external" {
            "http://elsewhere.example".into()
        } else {
            format!(
                "http://{}/next?x=%2B#top",
                req.headers()["host"].to_str().unwrap()
            )
        };
        (StatusCode::FOUND, [("location", dest)]).into_response()
    });
    let port = tunnel.upstream(V4, app).await;
    for (path, location) in [
        ("/local", format!("https://{HOST}/next?x=%2B#top")),
        ("/external", "http://elsewhere.example".into()),
    ] {
        let res = tunnel
            .connection
            .request_preview_http(
                request(Method::GET, "http", port, path)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::FOUND);
        assert_eq!(res.headers()["location"], location);
    }
    let res = tunnel
        .connection
        .request_preview_http(
            request(Method::GET, "http", port, "/gzip")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.headers()["content-encoding"], "gzip");
    assert_eq!(to_bytes(res.into_body(), 100).await.unwrap().as_ref(), GZIP);
}

#[tokio::test]
async fn denies_daemon_and_bad_ports_and_classifies_refused_connections() {
    let listener = TcpListener::bind((V4, 0)).await.unwrap();
    let own_port = listener.local_addr().unwrap().port();
    let tunnel = Tunnel::start(own_port).await;
    let unused = TcpListener::bind((V4, 0))
        .await
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    for port in [own_port, 22, 25, 4190, 6000, 6679, 2375, 9222, unused] {
        for kind in ["http", "ws"] {
            let req = request(Method::POST, kind, port, "/")
                .body(Body::empty())
                .unwrap();
            let res = if kind == "http" {
                tunnel.connection.request_preview_http(req).await
            } else {
                tunnel.connection.open_preview_ws(req).await
            }
            .unwrap();
            let (status, reason) = if port == unused {
                (502, "connection_refused")
            } else {
                (403, "port_forbidden")
            };
            assert_eq!(res.status(), status);
            assert_eq!(res.headers()["x-pontia-preview-error"], reason);
        }
    }
}

#[tokio::test]
async fn forwards_to_explicitly_allowed_sensitive_port() {
    let candidates = [9222, 9229, 8888, 8001, 6379, 11434, 2375, 2376]
        .map(|port| std::net::SocketAddr::new(V4, port));
    let listener = TcpListener::bind(candidates.as_slice()).await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let mut tunnel = Tunnel::with_config(
        8080,
        PreviewConfig {
            allowed_ports: vec![port],
            ..Default::default()
        },
    )
    .await;
    let app = Router::new().route("/", get(|| async { "preview" }));
    tunnel
        .tasks
        .spawn(async move { axum::serve(listener, app).await.unwrap() });
    let req = request(Method::GET, "http", port, "/")
        .body(Body::empty())
        .unwrap();
    let res = tunnel.connection.request_preview_http(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(to_bytes(res.into_body(), 128).await.unwrap(), "preview");
}

#[tokio::test]
async fn disabled_preview_rejects_http_and_ws_but_preserves_e2e() {
    let tunnel = Tunnel::with_config(
        0,
        PreviewConfig {
            enabled: false,
            allowed_ports: vec![9222],
        },
    )
    .await;
    for port in [5173, 9222] {
        let http = request(Method::GET, "http", port, "/")
            .body(Body::empty())
            .unwrap();
        let ws = request(Method::POST, "ws", port, "/")
            .body(Body::empty())
            .unwrap();
        assert_eq!(
            tunnel
                .connection
                .request_preview_http(http)
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            tunnel
                .connection
                .open_preview_ws(ws)
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
    }
    let res = tunnel
        .connection
        .request_e2e(
            Request::post("https://pontia-device/e2e/v1/requests")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::ACCEPTED);
}

#[tokio::test]
async fn websocket_negotiates_vite_protocol_and_transfers_messages_in_both_directions() {
    let mut tunnel = Tunnel::start(0).await;
    let app = Router::new().route(
        "/hmr",
        get(|headers: HeaderMap, ws: WebSocketUpgrade| async move {
            assert_eq!(
                headers["origin"],
                format!("http://{}", headers["host"].to_str().unwrap())
            );
            assert!(!headers.contains_key("x-pontia-preview-token"));
            assert_eq!(headers["cookie"], "app=test");
            ws.protocols(["vite-hmr"])
                .on_upgrade(|mut socket| async move {
                    socket.send(Message::Text("ready".into())).await.unwrap();
                    while let Some(Ok(msg)) = socket.recv().await {
                        if matches!(msg, Message::Close(_)) {
                            futures_util::SinkExt::flush(&mut socket).await.unwrap();
                            break;
                        }
                        socket.send(msg).await.unwrap();
                    }
                })
        }),
    );
    let port = tunnel.upstream(V4, app).await;
    let (tx, rx) = mpsc::channel(2);
    let req = request(Method::POST, "ws", port, "/hmr?token=test")
        .header("origin", format!("https://{HOST}"))
        .header("sec-websocket-protocol", "vite-hmr")
        .header("cookie", "app=test")
        .body(Body::from_stream(ReceiverStream::new(rx)))
        .unwrap();
    let res = tunnel.connection.open_preview_ws(req).await.unwrap();
    assert_eq!(res.status(), 200);
    assert_eq!(res.headers()["sec-websocket-protocol"], "vite-hmr");
    let mut body = res.into_body().into_data_stream();
    let mut buffer = BytesMut::new();
    let ready = WsTunnelFrame::Text("ready".into());
    for expected in [
        ready.clone(),
        WsTunnelFrame::Text("reload".into()),
        WsTunnelFrame::Binary(Bytes::from_static(b"\0\xff")),
        WsTunnelFrame::Close(Some((1000, "done".into()))),
    ] {
        if expected != ready {
            let bytes = expected.encode().unwrap();
            tx.send(Ok::<_, Infallible>(bytes.slice(..2)))
                .await
                .unwrap();
            tx.send(Ok(bytes.slice(2..))).await.unwrap();
        }
        let received = timeout(Duration::from_secs(5), async {
            loop {
                if let Some(frame) = WsTunnelFrame::decode(&mut buffer).unwrap() {
                    return frame;
                }
                buffer.extend_from_slice(&body.next().await.unwrap().unwrap());
            }
        })
        .await
        .unwrap();
        assert_eq!(received, expected);
    }
}

#[tokio::test]
async fn websocket_rejection_and_shutdown_reach_the_peer() {
    let mut tunnel = Tunnel::start(0).await;
    let (closed, mut observed) = mpsc::channel(1);
    let app = Router::new()
        .route("/deny", get(|| async { StatusCode::FORBIDDEN }))
        .route(
            "/idle",
            get(move |ws: WebSocketUpgrade| async move {
                ws.on_upgrade(move |mut ws| async move {
                    closed
                        .send(matches!(ws.recv().await, Some(Ok(Message::Close(_)))))
                        .await
                        .unwrap();
                })
            }),
        );
    let port = tunnel.upstream(V6, app).await;
    for (path, status) in [("/deny", 403), ("/idle", 200)] {
        let req = request(Method::POST, "ws", port, path)
            .body(Body::from_stream(stream::pending::<
                Result<Bytes, Infallible>,
            >()))
            .unwrap();
        let res = tunnel.connection.open_preview_ws(req).await.unwrap();
        assert_eq!(res.status(), status);
        if path == "/idle" {
            tunnel.shutdown.send(true).unwrap();
            assert_eq!(
                timeout(Duration::from_secs(5), observed.recv())
                    .await
                    .unwrap(),
                Some(true)
            );
        }
    }
}

#[tokio::test]
async fn oversized_tunnel_message_closes_both_sides_with_1009() {
    let mut tunnel = Tunnel::start(0).await;
    let (closed, mut observed) = mpsc::channel(1);
    let app = Router::new().route(
        "/",
        get(move |ws: WebSocketUpgrade| async move {
            ws.on_upgrade(move |mut socket| async move {
                if let Some(Ok(Message::Close(Some(close)))) = socket.recv().await {
                    closed.send(close.code).await.unwrap();
                }
            })
        }),
    );
    let port = tunnel.upstream(V4, app).await;
    let header = Bytes::from_static(b"\x02\x01\x00\x00\x01");
    let req = request(Method::POST, "ws", port, "/")
        .body(Body::from(header))
        .unwrap();
    let res = tunnel.connection.open_preview_ws(req).await.unwrap();
    assert_eq!(res.status(), 200);
    let bytes = timeout(Duration::from_secs(5), to_bytes(res.into_body(), 512))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        WsTunnelFrame::decode(&mut BytesMut::from(bytes.as_ref())).unwrap(),
        Some(WsTunnelFrame::Close(Some((1009, String::new()))))
    );
    assert_eq!(
        timeout(Duration::from_secs(5), observed.recv())
            .await
            .unwrap(),
        Some(1009)
    );
}
