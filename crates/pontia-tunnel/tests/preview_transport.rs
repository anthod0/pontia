use std::{convert::Infallible, time::Duration};

use axum::{
    Router,
    body::{Body, to_bytes},
    extract::WebSocketUpgrade,
    http::{Request, Response, StatusCode},
    routing::get,
};
use bytes::{Bytes, BytesMut};
use futures_util::{StreamExt, stream};
use pontia_tunnel::{
    DeviceRequestHandler, TunnelConnection, TunnelError, WsTunnelFrame, connect_edge, serve_device,
};
use tokio::{
    net::TcpListener,
    sync::{mpsc, watch},
    task::JoinSet,
    time::timeout,
};

struct Tunnel {
    connection: TunnelConnection,
    _tasks: JoinSet<()>,
    _shutdown: watch::Sender<bool>,
}

async fn connect(handler: DeviceRequestHandler) -> Tunnel {
    let mut tasks = JoinSet::new();
    let (tx, mut rx) = mpsc::channel(1);
    let app = Router::new().route(
        "/tunnel",
        get(move |ws: WebSocketUpgrade| async move {
            ws.on_upgrade(move |ws| async move {
                let (connection, runtime) = connect_edge(ws).await.unwrap();
                tx.send(connection).await.unwrap();
                let _ = runtime.run().await;
            })
        }),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tasks.spawn(async move { axum::serve(listener, app).await.unwrap() });
    let (ws, _) = tokio_tungstenite::connect_async(format!("ws://{address}/tunnel"))
        .await
        .unwrap();
    let (shutdown, stop) = watch::channel(false);
    tasks.spawn(async move {
        let _ = serve_device(ws, handler, stop).await;
    });
    Tunnel {
        connection: rx.recv().await.unwrap(),
        _tasks: tasks,
        _shutdown: shutdown,
    }
}

fn request(path: &str, body: Body) -> Request<Body> {
    Request::post(format!("https://pontia-device{path}"))
        .body(body)
        .unwrap()
}

const HTTP: &str = "/preview/v1/http/5173/";
const WS: &str = "/preview/v1/ws/5173/hmr";
const E2E: &str = "/e2e/v1/requests";

fn pending_body() -> Body {
    Body::from_stream(stream::pending::<Result<Bytes, Infallible>>())
}

#[tokio::test]
async fn saturated_preview_pools_leave_all_control_streams_available() {
    let tunnel = connect(DeviceRequestHandler::new(|_| async {
        Response::new(pending_body())
    }))
    .await;
    let conn = &tunnel.connection;
    let mut held = Vec::new();
    for _ in 0..128 {
        held.push(
            conn.request_preview_http(request(HTTP, Body::empty()))
                .await
                .unwrap(),
        );
    }
    assert!(matches!(
        conn.request_preview_http(request(HTTP, Body::empty()))
            .await,
        Err(TunnelError::Overloaded)
    ));
    for _ in 0..32 {
        held.push(
            conn.open_preview_ws(request(WS, Body::empty()))
                .await
                .unwrap(),
        );
    }
    assert!(matches!(
        conn.open_preview_ws(request(WS, Body::empty())).await,
        Err(TunnelError::Overloaded)
    ));
    for _ in 0..64 {
        let response = timeout(
            Duration::from_secs(2),
            conn.request_e2e(request(E2E, Body::empty())),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        held.push(response);
    }
    assert!(matches!(
        conn.request_e2e(request(E2E, Body::empty())).await,
        Err(TunnelError::Overloaded)
    ));
    drop(held);
    assert!(
        conn.request_preview_http(request(HTTP, Body::empty()))
            .await
            .is_ok()
    );
}

fn channel_body(rx: mpsc::Receiver<Bytes>) -> Body {
    Body::from_stream(stream::unfold(rx, |mut rx| async move {
        rx.recv()
            .await
            .map(|chunk| (Ok::<_, Infallible>(chunk), rx))
    }))
}

#[tokio::test]
async fn streams_sse_before_completion_and_large_responses() {
    let (send, recv) = mpsc::channel(1);
    let recv = std::sync::Mutex::new(Some(recv));
    let tunnel = connect(DeviceRequestHandler::new(move |req| {
        let sse = req.uri().query() == Some("events=1");
        let body = if sse {
            channel_body(recv.lock().unwrap().take().unwrap())
        } else {
            Body::from(vec![17; 5 * 1024 * 1024])
        };
        async move { Response::new(body) }
    }))
    .await;
    let conn = &tunnel.connection;
    let mut events = conn
        .request_preview_http(request(&format!("{HTTP}?events=1"), Body::empty()))
        .await
        .unwrap()
        .into_body()
        .into_data_stream();
    for value in ["data: one\n\n", "data: two\n\n"] {
        send.send(Bytes::from_static(value.as_bytes()))
            .await
            .unwrap();
        assert_eq!(
            timeout(Duration::from_secs(2), events.next())
                .await
                .unwrap()
                .unwrap()
                .unwrap(),
            value
        );
    }
    let response = conn
        .request_preview_http(request(HTTP, Body::empty()))
        .await
        .unwrap();
    assert_eq!(
        to_bytes(response.into_body(), 6 * 1024 * 1024)
            .await
            .unwrap(),
        &vec![17; 5 * 1024 * 1024][..]
    );
}

#[tokio::test]
async fn websocket_duplex_stream_survives_silent_upload_and_fragmentation() {
    let tunnel = connect(DeviceRequestHandler::new(|req| async move {
        assert_eq!(req.uri().path(), WS);
        Response::new(req.into_body())
    }))
    .await;
    let (send, recv) = mpsc::channel(1);
    let mut response = tunnel
        .connection
        .open_preview_ws(request(WS, channel_body(recv)))
        .await
        .unwrap()
        .into_body()
        .into_data_stream();
    tokio::time::sleep(Duration::from_secs(31)).await;
    let mut buffer = BytesMut::new();
    for frame in [
        WsTunnelFrame::Text("message".into()),
        WsTunnelFrame::Binary(Bytes::from_static(b"\0\xff")),
        WsTunnelFrame::Ping(Bytes::from_static(b"p")),
        WsTunnelFrame::Pong(Bytes::from_static(b"p")),
        WsTunnelFrame::Close(Some((1000, "bye".into()))),
    ] {
        for byte in frame.encode().unwrap() {
            send.send(Bytes::from(vec![byte])).await.unwrap();
        }
        let received = timeout(Duration::from_secs(2), async {
            loop {
                if let Some(frame) = WsTunnelFrame::decode(&mut buffer).unwrap() {
                    break frame;
                }
                buffer.extend_from_slice(&response.next().await.unwrap().unwrap());
            }
        })
        .await
        .unwrap();
        assert_eq!(received, frame);
    }
}

#[tokio::test]
async fn websocket_handshake_times_out_with_open_request_body() {
    let tunnel = connect(DeviceRequestHandler::new(|req| async move {
        let _open = req;
        std::future::pending().await
    }))
    .await;
    let result = timeout(
        Duration::from_secs(17),
        tunnel
            .connection
            .open_preview_ws(request(WS, pending_body())),
    )
    .await
    .unwrap();
    assert!(matches!(result, Err(TunnelError::Timeout)));
}

#[tokio::test]
async fn dispatch_checks_route_method_and_directional_metadata() {
    let tunnel = connect(DeviceRequestHandler::new(|req| async move {
        assert_eq!(req.method(), "OPTIONS");
        assert_eq!(req.uri(), "/preview/v1/http/5173/path?q=1");
        assert_eq!(req.headers()["x-pontia-preview-host"], "preview.example");
        assert!(!req.headers().contains_key("x-pontia-preview-error"));
        Response::builder()
            .status(403)
            .header("x-pontia-preview-error", "port_forbidden")
            .header("x-pontia-preview-host", "wrong-direction")
            .body(Body::empty())
            .unwrap()
    }))
    .await;
    let conn = &tunnel.connection;
    let req = Request::builder()
        .method("OPTIONS")
        .uri(format!("https://pontia-device{HTTP}path?q=1"))
        .header("x-pontia-preview-host", "preview.example")
        .header("x-pontia-preview-error", "spoof")
        .body(Body::empty())
        .unwrap();
    let response = conn.request_preview_http(req).await.unwrap();
    assert_eq!(response.status(), 403);
    assert_eq!(
        response.headers()["x-pontia-preview-error"],
        "port_forbidden"
    );
    assert!(!response.headers().contains_key("x-pontia-preview-host"));
    let invalid = Request::get(format!("https://pontia-device{WS}"))
        .body(Body::empty())
        .unwrap();
    assert_eq!(conn.open_preview_ws(invalid).await.unwrap().status(), 400);
    assert!(matches!(
        conn.request(request(HTTP, Body::empty())).await,
        Err(TunnelError::InvalidRequest)
    ));
}
