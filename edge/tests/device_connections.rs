mod support;

use std::time::Duration;

use futures_util::SinkExt;
use pontia_tunnel::{
    DeviceIdentity, RemoteClient,
    protocol::{self, Message},
};
use support::{TestEdge, closed, receive, send, wait_for};
use tokio::sync::watch;
use tokio_tungstenite::{connect_async_tls_with_config, tungstenite::Message as WsMessage};

#[tokio::test]
async fn authenticated_device_heartbeats_and_disconnects() {
    let server = TestEdge::start().await;
    let identity = DeviceIdentity::generate().unwrap();
    server.bind(&identity).await;
    let mut socket = server.authenticate(&identity).await;
    assert!(
        server
            .edge
            .online()
            .connection_id(identity.device_id())
            .is_some()
    );
    for _ in 0..3 {
        let Message::Ping { nonce } = receive(&mut socket).await else {
            panic!()
        };
        send(&mut socket, Message::Pong { nonce }).await;
    }
    socket.close(None).await.unwrap();
    wait_for(|| {
        server
            .edge
            .online()
            .connection_id(identity.device_id())
            .is_none()
    })
    .await;
}

#[tokio::test]
async fn unknown_device_invalid_signature_and_replay_never_become_online() {
    let server = TestEdge::start().await;
    let identity = DeviceIdentity::generate().unwrap();
    let mut first = server.connect().await;
    let Message::Challenge { nonce, .. } = receive(&mut first).await else {
        panic!()
    };
    send(&mut first, identity.authenticate(&nonce)).await;
    closed(&mut first).await;
    assert!(
        server
            .edge
            .online()
            .connection_id(identity.device_id())
            .is_none()
    );

    server.bind(&identity).await;
    let mut replay = server.connect().await;
    let Message::Challenge { nonce: fresh, .. } = receive(&mut replay).await else {
        panic!()
    };
    assert_ne!(nonce, fresh);
    send(&mut replay, identity.authenticate(&nonce)).await;
    closed(&mut replay).await;
    assert!(
        server
            .edge
            .online()
            .connection_id(identity.device_id())
            .is_none()
    );

    let mut wrong_key = server.connect().await;
    let Message::Challenge { nonce, .. } = receive(&mut wrong_key).await else {
        panic!()
    };
    let Message::Authenticate { signature, .. } =
        DeviceIdentity::generate().unwrap().authenticate(&nonce)
    else {
        panic!()
    };
    send(
        &mut wrong_key,
        Message::Authenticate {
            device_id: identity.device_id(),
            signature,
        },
    )
    .await;
    closed(&mut wrong_key).await;
    assert!(
        server
            .edge
            .online()
            .connection_id(identity.device_id())
            .is_none()
    );
}

#[tokio::test]
async fn pending_connections_are_bounded_and_time_out() {
    let server = TestEdge::start().await;
    let mut first = server.connect().await;
    receive(&mut first).await;
    let mut second = server.connect().await;
    receive(&mut second).await;
    let rejected =
        connect_async_tls_with_config(&server.url, None, false, Some(server.connector.clone()))
            .await;
    assert!(
        matches!(rejected, Err(tokio_tungstenite::tungstenite::Error::Http(response)) if response.status() == 503)
    );
    closed(&mut first).await;
    closed(&mut second).await;
    let identity = DeviceIdentity::generate().unwrap();
    server.bind(&identity).await;
    let mut socket = server.authenticate(&identity).await;
    socket.close(None).await.unwrap();
}

#[tokio::test]
async fn premature_business_frames_and_oversized_messages_are_rejected() {
    let server = TestEdge::start().await;
    for body in [
        r#"{"type":"open","stream_id":1}"#.to_owned(),
        "x".repeat(protocol::MAX_MESSAGE_BYTES + 1),
    ] {
        let mut socket = server.connect().await;
        receive(&mut socket).await;
        socket.send(WsMessage::Text(body.into())).await.unwrap();
        closed(&mut socket).await;
    }
}

#[tokio::test]
async fn missing_or_incorrect_pong_removes_online_device() {
    let server = TestEdge::start().await;
    let identity = DeviceIdentity::generate().unwrap();
    server.bind(&identity).await;
    for wrong_pong in [false, true] {
        let mut socket = server.authenticate(&identity).await;
        assert!(matches!(receive(&mut socket).await, Message::Ping { .. }));
        if wrong_pong {
            send(&mut socket, Message::Pong { nonce: [0; 32] }).await;
        }
        closed(&mut socket).await;
        wait_for(|| {
            server
                .edge
                .online()
                .connection_id(identity.device_id())
                .is_none()
        })
        .await;
    }
}

#[tokio::test]
async fn new_authenticated_connection_replaces_old_without_losing_online_record() {
    let server = TestEdge::start().await;
    let identity = DeviceIdentity::generate().unwrap();
    server.bind(&identity).await;
    let mut old = server.authenticate(&identity).await;
    let old_id = server
        .edge
        .online()
        .connection_id(identity.device_id())
        .unwrap();
    let mut new = server.authenticate(&identity).await;
    let new_id = server
        .edge
        .online()
        .connection_id(identity.device_id())
        .unwrap();
    assert_ne!(old_id, new_id);
    closed(&mut old).await;
    assert_eq!(
        server.edge.online().connection_id(identity.device_id()),
        Some(new_id)
    );
    let Message::Ping { nonce } = receive(&mut new).await else {
        panic!()
    };
    send(&mut new, Message::Pong { nonce }).await;
    new.close(None).await.unwrap();
}

#[tokio::test]
async fn production_client_reconnects_reauthenticates_and_stops() {
    let server = TestEdge::start().await;
    let path = server.root.path().join("device/identity.json");
    let identity = DeviceIdentity::load_or_create(&path).unwrap();
    let id = identity.device_id();
    server.bind(&identity).await;
    let client = RemoteClient::new(&server.url, identity, Some(&server.ca_path)).unwrap();
    let (stop, shutdown) = watch::channel(false);
    let task = tokio::spawn(client.run(shutdown));
    wait_for(|| server.edge.online().connection_id(id).is_some()).await;
    let first = server.edge.online().connection_id(id).unwrap();
    // A second authenticated connection forces the production client through reconnection.
    let duplicate = DeviceIdentity::load_or_create(&path).unwrap();
    let mut replacement = server.authenticate(&duplicate).await;
    let second = server.edge.online().connection_id(id).unwrap();
    assert_ne!(first, second);
    replacement.close(None).await.unwrap();
    wait_for(|| {
        server
            .edge
            .online()
            .connection_id(id)
            .is_some_and(|value| value != first && value != second)
    })
    .await;
    let third = server.edge.online().connection_id(id).unwrap();
    tokio::time::sleep(Duration::from_millis(700)).await;
    assert_eq!(
        server.edge.online().connection_id(id),
        Some(third),
        "client must answer heartbeats"
    );
    stop.send_replace(true);
    tokio::time::timeout(Duration::from_secs(1), task)
        .await
        .unwrap()
        .unwrap();
    wait_for(|| server.edge.online().connection_id(id).is_none()).await;
}

#[tokio::test]
async fn client_checks_server_certificate_and_can_stop_while_retrying() {
    let server = TestEdge::start().await;
    let rejected = tokio::time::timeout(
        Duration::from_secs(3),
        connect_async_tls_with_config(&server.url, None, false, None),
    )
    .await
    .unwrap();
    let error = rejected.unwrap_err();
    assert!(error.to_string().contains("UnknownIssuer"), "{error:?}");
    let identity = DeviceIdentity::generate().unwrap();
    let id = identity.device_id();
    server.bind(&identity).await;
    let client = RemoteClient::new(&server.url, identity, None).unwrap();
    let (stop, shutdown) = watch::channel(false);
    let task = tokio::spawn(client.run(shutdown));
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(server.edge.online().connection_id(id).is_none());
    stop.send_replace(true);
    tokio::time::timeout(Duration::from_secs(1), task)
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn edge_shutdown_closes_pending_and_authenticated_connections() {
    let server = TestEdge::start().await;
    let identity = DeviceIdentity::generate().unwrap();
    server.bind(&identity).await;
    let mut online = server.authenticate(&identity).await;
    let mut pending = server.connect().await;
    receive(&mut pending).await;
    server.edge.shutdown();
    closed(&mut online).await;
    closed(&mut pending).await;
    wait_for(|| {
        server
            .edge
            .online()
            .connection_id(identity.device_id())
            .is_none()
    })
    .await;
}
