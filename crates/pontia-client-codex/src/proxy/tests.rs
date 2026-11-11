use super::*;
use futures_util::{SinkExt, StreamExt};
use tokio::time::{Duration, timeout};
use tokio_tungstenite::{accept_async, client_async, tungstenite::Message};

type WebSocket = tokio_tungstenite::WebSocketStream<UnixStream>;

async fn connect(path: &Path) -> WebSocket {
    client_async("ws://localhost/", UnixStream::connect(path).await.unwrap())
        .await
        .unwrap()
        .0
}

async fn receive(socket: &mut WebSocket) -> Message {
    timeout(Duration::from_secs(3), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap()
}

#[tokio::test]
async fn forwards_handshake_unknown_messages_and_interactions_on_independent_connections() {
    let root = tempfile::tempdir().unwrap();
    let upstream_path = root.path().join("native.sock");
    let upstream = UnixListener::bind(&upstream_path).unwrap();
    let proxy = Proxy::bind(root.path()).await.unwrap();
    let path = proxy.path.clone();
    let (shutdown, stop) = watch::channel(false);
    let (observations, _incoming) = mpsc::channel(128);
    let task = tokio::spawn(async move {
        proxy
            .run(stop, move || Ok(upstream_path.clone()), observations)
            .await
    });
    let server = tokio::spawn(async move {
        let mut peers = Vec::new();
        for _ in 0..2 {
            peers.push(
                accept_async(upstream.accept().await.unwrap().0)
                    .await
                    .unwrap(),
            );
        }
        peers
    });
    let mut first = connect(&path).await;
    let mut second = connect(&path).await;
    let mut peers = server.await.unwrap();
    for (index, client) in [&mut first, &mut second].into_iter().enumerate() {
        let request = Message::text(format!(
            r#"{{"id":1,"method":"future/request","params":{{"connection":{index},"unknown":[null,true]}}}}"#
        ));
        client.send(request.clone()).await.unwrap();
        assert_eq!(receive(&mut peers[index]).await, request);
        let messages = [
            Message::text(format!(r#"{{"id":1,"result":{{"connection":{index}}}}}"#)),
            Message::text(
                r#"{"id":"approval","method":"future/interaction","params":{"unknown":true}}"#,
            ),
            Message::text(r#"{"method":"future/delta","params":{"text":"stream"}}"#),
        ];
        for message in &messages {
            peers[index].send(message.clone()).await.unwrap();
        }
        for message in messages {
            assert_eq!(receive(client).await, message);
        }
        let reply =
            Message::text(r#"{"id":"approval","result":{"decision":"accept","unknown":42}}"#);
        client.send(reply.clone()).await.unwrap();
        assert_eq!(receive(&mut peers[index]).await, reply);
    }
    drop(first);
    assert!(
        timeout(Duration::from_secs(3), peers[0].next())
            .await
            .unwrap()
            .unwrap()
            .is_err()
    );
    let payload = Message::text("x".repeat(1024 * 1024));
    let (sent, received) = tokio::join!(second.send(payload.clone()), receive(&mut peers[1]));
    sent.unwrap();
    assert_eq!(received, payload);
    shutdown.send(true).unwrap();
    task.await.unwrap().unwrap();
    assert!(!path.exists());
    assert!(
        timeout(Duration::from_secs(3), second.next())
            .await
            .unwrap()
            .unwrap()
            .is_err()
    );
}

#[tokio::test]
async fn upstream_disconnect_and_unavailability_close_only_the_affected_transport() {
    let root = tempfile::tempdir().unwrap();
    let upstream_path = root.path().join("native.sock");
    let upstream = UnixListener::bind(&upstream_path).unwrap();
    let proxy = Proxy::bind(root.path()).await.unwrap();
    let path = proxy.path.clone();
    let (shutdown, stop) = watch::channel(false);
    let (observations, _incoming) = mpsc::channel(128);
    let task = tokio::spawn(async move {
        proxy
            .run(stop, move || Ok(upstream_path.clone()), observations)
            .await
    });
    let mut client = connect(&path).await;
    let mut native = accept_async(upstream.accept().await.unwrap().0)
        .await
        .unwrap();
    native.send(Message::text("hello")).await.unwrap();
    assert_eq!(receive(&mut client).await, Message::text("hello"));
    drop(native);
    assert!(
        timeout(Duration::from_secs(3), client.next())
            .await
            .unwrap()
            .unwrap()
            .is_err()
    );
    drop(upstream);
    let mut client = connect(&path).await;
    assert!(
        timeout(Duration::from_secs(3), client.next())
            .await
            .unwrap()
            .unwrap()
            .is_err()
    );
    shutdown.send(true).unwrap();
    task.await.unwrap().unwrap();
}

#[tokio::test]
async fn reclaims_stale_sockets_but_preserves_live_listeners_and_other_files() {
    let root = tempfile::tempdir().unwrap();
    let path = socket_path(root.path());
    std::fs::write(&path, b"keep").unwrap();
    assert!(Proxy::bind(root.path()).await.is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"keep");
    std::fs::remove_file(&path).unwrap();
    drop(UnixListener::bind(&path).unwrap());
    let proxy = Proxy::bind(root.path()).await.unwrap();
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert!(Proxy::bind(root.path()).await.is_err());
    drop(proxy);
    assert!(!path.exists());
}

#[tokio::test]
async fn only_matching_successful_thread_responses_are_observed() {
    let root = tempfile::tempdir().unwrap();
    let upstream_path = root.path().join("native.sock");
    let upstream = UnixListener::bind(&upstream_path).unwrap();
    let proxy = Proxy::bind(root.path()).await.unwrap();
    let path = proxy.path.clone();
    let (shutdown, stop) = watch::channel(false);
    let (observations, mut incoming) = mpsc::channel(128);
    let task = tokio::spawn(async move {
        proxy
            .run(stop, move || Ok(upstream_path.clone()), observations)
            .await
    });
    let mut client = connect(&path).await;
    let mut native = accept_async(upstream.accept().await.unwrap().0)
        .await
        .unwrap();
    for request in [
        serde_json::json!({"id":1,"method":"thread/start"}),
        serde_json::json!({"id":"1","method":"thread/resume"}),
        serde_json::json!({"id":2,"method":"thread/fork"}),
    ] {
        client
            .send(Message::text(request.to_string()))
            .await
            .unwrap();
        receive(&mut native).await;
    }
    for response in [
        serde_json::json!({"id":1,"method":"future/serverRequest"}),
        serde_json::json!({"id":2,"error":{"code":-32600}}),
        serde_json::json!({"id":"1","result":{"thread":{"id":"resumed"}}}),
        serde_json::json!({"id":1,"result":{"thread":{"id":"started"}}}),
        serde_json::json!({"id":99,"result":{"thread":{"id":"unsolicited"}}}),
    ] {
        let message = Message::text(response.to_string());
        native.send(message.clone()).await.unwrap();
        assert_eq!(receive(&mut client).await, message);
    }
    for expected in ["resumed", "started"] {
        let observation = incoming.recv().await.unwrap();
        let ObservedEvent::ThreadOpened { result, .. } = observation.event else {
            panic!("unexpected event")
        };
        assert_eq!(result["thread"]["id"], expected);
    }
    assert!(incoming.try_recv().is_err());
    shutdown.send(true).unwrap();
    task.await.unwrap().unwrap();
}
