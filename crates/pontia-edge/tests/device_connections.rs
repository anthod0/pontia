mod support;

use std::time::Duration;

use futures_util::SinkExt;
use pontia_tunnel::{
    RemoteClient,
    protocol::{self, Message},
};
use support::{CLI_CREDENTIAL, TestEdge, closed, receive, send, wait_for};
use tokio::sync::watch;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use uuid::Uuid;

#[tokio::test]
async fn bearer_ticket_connects_heartbeats_and_disconnects() {
    let server = TestEdge::start().await;
    let device_id = Uuid::new_v4();
    let mut socket = server.authenticate(device_id).await;
    wait_for(|| server.edge.online().connection_id(device_id).is_some()).await;
    for _ in 0..3 {
        let Message::Ping { nonce } = receive(&mut socket).await else {
            panic!("expected ping")
        };
        send(&mut socket, Message::Pong { nonce }).await;
    }
    socket.close(None).await.unwrap();
    wait_for(|| server.edge.online().connection_id(device_id).is_none()).await;
}

#[tokio::test]
async fn missing_invalid_and_replayed_tickets_are_unauthorized() {
    let server = TestEdge::start().await;
    let device_id = Uuid::new_v4();
    let ticket = server.issue_ticket(device_id);

    for candidate in [None, Some("invalid-ticket")] {
        let rejected = server.connect(candidate).await.unwrap_err();
        assert!(
            matches!(rejected, tokio_tungstenite::tungstenite::Error::Http(response)
            if response.status() == 401
                && response.headers().get("www-authenticate").unwrap() == "Bearer")
        );
    }

    let mut accepted = server.connect(Some(&ticket)).await.unwrap();
    accepted.close(None).await.unwrap();
    let replayed = server.connect(Some(&ticket)).await.unwrap_err();
    assert!(
        matches!(replayed, tokio_tungstenite::tungstenite::Error::Http(response) if response.status() == 401)
    );
}

#[tokio::test]
async fn pending_ticket_redemptions_are_bounded_before_consumption() {
    let server = TestEdge::start().await;
    server.delay_redemption();
    let first_ticket = server.issue_ticket(Uuid::new_v4());
    let second_ticket = server.issue_ticket(Uuid::new_v4());
    let pending = async {
        tokio::join!(
            server.connect(Some(&first_ticket)),
            server.connect(Some(&second_ticket)),
        )
    };
    tokio::pin!(pending);
    tokio::select! {
        _ = tokio::time::sleep(Duration::from_millis(50)) => {}
        _ = &mut pending => panic!("redemptions should still be pending"),
    }

    let rejected = server
        .connect(Some(&server.issue_ticket(Uuid::new_v4())))
        .await
        .unwrap_err();
    assert!(
        matches!(rejected, tokio_tungstenite::tungstenite::Error::Http(response) if response.status() == 503)
    );
    let (first, second) = pending.await;
    let mut first = first.unwrap();
    let mut second = second.unwrap();
    first.close(None).await.unwrap();
    second.close(None).await.unwrap();
}

#[tokio::test]
async fn website_failure_blocks_new_connections_but_not_existing_ones() {
    let server = TestEdge::start().await;
    let device_id = Uuid::new_v4();
    let mut existing = server.authenticate(device_id).await;
    wait_for(|| server.edge.online().connection_id(device_id).is_some()).await;
    server.stop_website();

    let rejected = server
        .connect(Some(&server.issue_ticket(Uuid::new_v4())))
        .await
        .unwrap_err();
    assert!(
        matches!(rejected, tokio_tungstenite::tungstenite::Error::Http(response) if response.status() == 503)
    );

    let Message::Ping { nonce } = receive(&mut existing).await else {
        panic!("expected ping")
    };
    send(&mut existing, Message::Pong { nonce }).await;
    assert!(server.edge.online().connection_id(device_id).is_some());
    existing.close(None).await.unwrap();
}

#[tokio::test]
async fn oversized_messages_and_incorrect_pongs_remove_online_devices() {
    let server = TestEdge::start().await;
    for wrong_pong in [false, true] {
        let device_id = Uuid::new_v4();
        let mut socket = server.authenticate(device_id).await;
        wait_for(|| server.edge.online().connection_id(device_id).is_some()).await;
        let Message::Ping { .. } = receive(&mut socket).await else {
            panic!("expected ping")
        };
        if wrong_pong {
            send(&mut socket, Message::Pong { nonce: [0; 32] }).await;
        }
        closed(&mut socket).await;
        wait_for(|| server.edge.online().connection_id(device_id).is_none()).await;
    }

    let device_id = Uuid::new_v4();
    let mut socket = server.authenticate(device_id).await;
    socket
        .send(WsMessage::Text(
            "x".repeat(protocol::MAX_MESSAGE_BYTES + 1).into(),
        ))
        .await
        .unwrap();
    closed(&mut socket).await;
}

#[tokio::test]
async fn new_connection_replaces_old_without_losing_new_online_record() {
    let server = TestEdge::start().await;
    let device_id = Uuid::new_v4();
    let mut old = server.authenticate(device_id).await;
    wait_for(|| server.edge.online().connection_id(device_id).is_some()).await;
    let old_id = server.edge.online().connection_id(device_id).unwrap();
    let mut new = server.authenticate(device_id).await;
    wait_for(|| server.edge.online().connection_id(device_id) != Some(old_id)).await;
    let new_id = server.edge.online().connection_id(device_id).unwrap();
    closed(&mut old).await;
    assert_eq!(server.edge.online().connection_id(device_id), Some(new_id));
    let Message::Ping { nonce } = receive(&mut new).await else {
        panic!("expected ping")
    };
    send(&mut new, Message::Pong { nonce }).await;
    new.close(None).await.unwrap();
}

#[tokio::test]
async fn production_client_fetches_a_fresh_ticket_when_reconnecting() {
    let server = TestEdge::start().await;
    let device_id = Uuid::new_v4();
    let home = server.root.path().join("device");
    std::fs::create_dir(&home).unwrap();
    std::fs::write(
        home.join("auth.json"),
        serde_json::json!({ "token": CLI_CREDENTIAL }).to_string(),
    )
    .unwrap();
    let client = RemoteClient::with_clients(
        &server.website_origin,
        device_id,
        &home,
        server.http.clone(),
        server.connector.clone(),
    )
    .unwrap();
    let (stop, shutdown) = watch::channel(false);
    let task = tokio::spawn(client.run(shutdown));
    wait_for(|| server.edge.online().connection_id(device_id).is_some()).await;
    let first = server.edge.online().connection_id(device_id).unwrap();
    assert_eq!(server.issued_count(), 1);

    let mut replacement = server.authenticate(device_id).await;
    wait_for(|| server.edge.online().connection_id(device_id) != Some(first)).await;
    replacement.close(None).await.unwrap();
    wait_for(|| {
        server.issued_count() >= 2 && server.edge.online().connection_id(device_id).is_some()
    })
    .await;
    tokio::time::sleep(Duration::from_millis(350)).await;
    assert!(
        server.edge.online().connection_id(device_id).is_some(),
        "client must answer heartbeats"
    );

    stop.send_replace(true);
    tokio::time::timeout(Duration::from_secs(1), task)
        .await
        .unwrap()
        .unwrap();
    wait_for(|| server.edge.online().connection_id(device_id).is_none()).await;
}

#[tokio::test]
async fn edge_shutdown_closes_authenticated_connections() {
    let server = TestEdge::start().await;
    let device_id = Uuid::new_v4();
    let mut socket = server.authenticate(device_id).await;
    wait_for(|| server.edge.online().connection_id(device_id).is_some()).await;
    server.edge.shutdown();
    closed(&mut socket).await;
    wait_for(|| server.edge.online().connection_id(device_id).is_none()).await;
}
