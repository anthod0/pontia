use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use axum::{
    Router,
    body::Body,
    extract::{State, WebSocketUpgrade},
    http::{Request, Response, StatusCode},
    routing::get,
};
use pontia_tunnel::{DeviceRequestHandler, TunnelConnection, connect_edge, serve_device};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::{mpsc, watch},
    task::JoinSet,
    time::timeout,
};

async fn check_request(connection: &TunnelConnection) {
    let request = Request::builder()
        .method("POST")
        .uri("https://pontia-device/e2e/v1/sessions")
        .header("content-type", "application/pontia-e2e")
        .body(Body::empty())
        .unwrap();
    let response = timeout(Duration::from_secs(5), connection.request(request))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

async fn relay(device: TcpStream, edge: TcpStream, discard: Arc<AtomicBool>) {
    let (mut device_reader, mut device_writer) = device.into_split();
    let (mut edge_reader, mut edge_writer) = edge.into_split();
    let upload = async {
        let mut buffer = [0; 65536];
        loop {
            let count = device_reader.read(&mut buffer).await.unwrap();
            if count == 0 {
                return;
            }
            if !discard.load(Ordering::SeqCst) {
                edge_writer.write_all(&buffer[..count]).await.unwrap();
            }
        }
    };
    let download = async {
        let mut buffer = [0; 65536];
        loop {
            let count = edge_reader.read(&mut buffer).await.unwrap();
            if count == 0 {
                // A local proxy can retain its device socket after losing the upstream.
                std::future::pending::<()>().await;
            }
            if !discard.load(Ordering::SeqCst) {
                device_writer.write_all(&buffer[..count]).await.unwrap();
            }
        }
    };
    tokio::join!(upload, download);
}

#[tokio::test]
async fn device_detects_a_silent_tunnel_without_a_tcp_disconnect() {
    let mut tasks = JoinSet::new();
    let (ready, mut connected) = mpsc::channel(1);
    let app = Router::new()
        .route(
            "/tunnel",
            get(
                |State(ready): State<mpsc::Sender<TunnelConnection>>, ws: WebSocketUpgrade| async {
                    ws.on_upgrade(move |socket| async move {
                        let (connection, runtime) = connect_edge(socket).await.unwrap();
                        ready.send(connection).await.unwrap();
                        let _ = runtime.run().await;
                    })
                },
            ),
        )
        .with_state(ready);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let edge_address = listener.local_addr().unwrap();
    tasks.spawn(async move { axum::serve(listener, app).await.unwrap() });

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_address = listener.local_addr().unwrap();
    let discard = Arc::new(AtomicBool::new(false));
    let proxy_discard = discard.clone();
    tasks.spawn(async move {
        let (device, _) = listener.accept().await.unwrap();
        let edge = TcpStream::connect(edge_address).await.unwrap();
        relay(device, edge, proxy_discard).await;
    });

    let (socket, _) = tokio_tungstenite::connect_async(format!("ws://{proxy_address}/tunnel"))
        .await
        .unwrap();
    let (_stop, shutdown) = watch::channel(false);
    let handler = DeviceRequestHandler::new(|_| async { Response::new(Body::empty()) });
    let device = serve_device(socket, handler, shutdown);
    tokio::pin!(device);
    let scenario = async {
        let connection = timeout(Duration::from_secs(5), connected.recv())
            .await
            .unwrap()
            .unwrap();
        check_request(&connection).await;
        tokio::time::sleep(Duration::from_secs(30)).await;
        check_request(&connection).await;
        discard.store(true, Ordering::SeqCst);
        connection
    };
    let _connection = tokio::select! {
        result = &mut device => panic!("healthy idle tunnel ended: {result:?}"),
        connection = scenario => connection,
    };

    let result = timeout(Duration::from_secs(30), &mut device)
        .await
        .expect("device must detect a silent tunnel and release the reconnect loop");
    assert!(
        tasks.try_join_next().is_none(),
        "the proxy must retain its sockets throughout the outage"
    );
    assert!(result.is_err());
}
