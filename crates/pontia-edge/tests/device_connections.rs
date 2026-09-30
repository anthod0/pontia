mod support;

use std::time::Duration;

use axum::{body::Body, http::Response};
use futures_util::SinkExt;
use pontia_tunnel::{DeviceRequestHandler, RemoteClient};
use support::{CLI_CREDENTIAL, TestEdge, protocol_closed, wait_for};
use tokio::sync::watch;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use uuid::Uuid;

fn ok_handler() -> DeviceRequestHandler {
    DeviceRequestHandler::new(|_| async { Response::new(Body::from("ok")) })
}

#[tokio::test]
async fn missing_wrong_and_unselected_subprotocols_are_rejected() {
    let server = TestEdge::start().await;
    for protocol in [None, Some("unknown-tunnel-v1")] {
        let ticket = server.issue_ticket(Uuid::new_v4());
        let rejected = server
            .connect_with_protocol(Some(&ticket), protocol)
            .await
            .unwrap_err();
        assert!(
            matches!(rejected, tokio_tungstenite::tungstenite::Error::Http(response) if response.status() == 400)
        );
    }
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
    first.unwrap().close(None).await.unwrap();
    second.unwrap().close(None).await.unwrap();
}

#[tokio::test]
async fn http2_ready_connection_stays_online_and_invalid_message_closes_connection() {
    let server = TestEdge::start().await;
    let device_id = Uuid::new_v4();
    let device = server
        .device(device_id, |_| async { Response::new(Body::from("ok")) })
        .await;
    assert!(server.edge.online().connection_id(device_id).is_some());
    drop(device);
    wait_for(|| server.edge.online().connection_id(device_id).is_none()).await;

    let mut raw = server.authenticate(Uuid::new_v4()).await;
    raw.send(WsMessage::Text("not-binary".into()))
        .await
        .unwrap();
    protocol_closed(&mut raw).await;
}

#[tokio::test]
async fn connection_that_is_not_http2_ready_does_not_replace_healthy_connection() {
    let server = TestEdge::start().await;
    let device_id = Uuid::new_v4();
    let _healthy = server
        .device(device_id, |_| async { Response::new(Body::from("ok")) })
        .await;
    let healthy_id = server.edge.online().connection_id(device_id).unwrap();
    let mut incomplete = server.authenticate(device_id).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(
        server.edge.online().connection_id(device_id),
        Some(healthy_id)
    );
    incomplete.close(None).await.unwrap();
}

#[tokio::test]
async fn new_ready_connection_replaces_old_without_losing_new_online_record() {
    let server = TestEdge::start().await;
    let device_id = Uuid::new_v4();
    let old = server
        .device(device_id, |_| async { Response::new(Body::from("old")) })
        .await;
    let old_id = server.edge.online().connection_id(device_id).unwrap();
    let new = server
        .device(device_id, |_| async { Response::new(Body::from("new")) })
        .await;
    wait_for(|| server.edge.online().connection_id(device_id) != Some(old_id)).await;
    let new_id = server.edge.online().connection_id(device_id).unwrap();
    drop(old);
    assert_eq!(server.edge.online().connection_id(device_id), Some(new_id));
    drop(new);
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
        ok_handler(),
    )
    .unwrap();
    let (stop, shutdown) = watch::channel(false);
    let task = tokio::spawn(client.run(shutdown));
    wait_for(|| server.edge.online().connection_id(device_id).is_some()).await;
    let first = server.edge.online().connection_id(device_id).unwrap();
    assert_eq!(server.issued_count(), 1);

    let replacement = server
        .device(device_id, |_| async {
            Response::new(Body::from("replacement"))
        })
        .await;
    wait_for(|| server.edge.online().connection_id(device_id) != Some(first)).await;
    drop(replacement);
    wait_for(|| {
        server.issued_count() >= 2 && server.edge.online().connection_id(device_id).is_some()
    })
    .await;

    stop.send_replace(true);
    tokio::time::timeout(Duration::from_secs(6), task)
        .await
        .unwrap()
        .unwrap();
    wait_for(|| server.edge.online().connection_id(device_id).is_none()).await;
}

#[tokio::test]
async fn edge_shutdown_closes_authenticated_connections() {
    let server = TestEdge::start().await;
    let device_id = Uuid::new_v4();
    let _device = server
        .device(device_id, |_| async { Response::new(Body::from("ok")) })
        .await;
    server.edge.shutdown();
    wait_for(|| server.edge.online().connection_id(device_id).is_none()).await;
}
