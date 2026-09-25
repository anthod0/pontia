use anyhow::{Result, bail};
use axum::{
    extract::{
        State, WebSocketUpgrade,
        ws::{Message as WsMessage, WebSocket},
    },
    http::StatusCode,
    response::Response,
};
use pontia_tunnel::protocol::{self, Message};
use tokio::{sync::OwnedSemaphorePermit, time::timeout};
use tracing::{info, warn};
use uuid::Uuid;

use crate::Edge;

pub(crate) async fn upgrade(
    State(edge): State<Edge>,
    ws: WebSocketUpgrade,
) -> Result<Response, StatusCode> {
    let permit = edge
        .pending
        .clone()
        .try_acquire_owned()
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    Ok(ws
        .max_message_size(protocol::MAX_MESSAGE_BYTES)
        .max_frame_size(protocol::MAX_MESSAGE_BYTES)
        .on_upgrade(move |socket| connection(edge, socket, permit)))
}

async fn connection(edge: Edge, mut socket: WebSocket, permit: OwnedSemaphorePermit) {
    let mut shutdown = edge.shutdown.subscribe();
    tokio::select! {
        biased;
        _ = shutdown.wait_for(|stop| *stop) => {}
        result = authenticated_connection(&edge, &mut socket, permit) => {
            if let Err(error) = result {
                warn!(%error, "device connection closed");
            }
        }
    }
    // Dropping the socket also terminates peers that never read a close frame.
}

async fn authenticated_connection(
    edge: &Edge,
    socket: &mut WebSocket,
    permit: OwnedSemaphorePermit,
) -> Result<()> {
    let device_id = timeout(edge.limits.auth_timeout, authenticate(edge, socket)).await??;
    drop(permit);
    let (_lease, mut replaced) = edge.online.register(device_id);
    tokio::select! {
        biased;
        _ = replaced.wait_for(|stop| *stop) => {}
        result = async {
            timeout(edge.limits.auth_timeout, send(socket, Message::Authenticated { device_id })).await??;
            info!(%device_id, "device online");
            heartbeat(edge, socket).await
        } => { result?; }
    }
    info!(%device_id, "device disconnected");
    Ok(())
}

async fn authenticate(edge: &Edge, socket: &mut WebSocket) -> Result<Uuid> {
    let nonce = protocol::nonce()?;
    send(
        socket,
        Message::Challenge {
            version: protocol::VERSION,
            nonce,
        },
    )
    .await?;
    let Message::Authenticate {
        device_id,
        signature,
    } = receive(socket).await?
    else {
        bail!("expected device authentication");
    };
    let public_key = edge
        .devices
        .public_key(device_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("device access denied"))?;
    protocol::verify(&public_key, device_id, &nonce, &signature)?;
    Ok(device_id)
}

async fn heartbeat(edge: &Edge, socket: &mut WebSocket) -> Result<()> {
    loop {
        tokio::select! {
            _ = tokio::time::sleep(edge.limits.heartbeat_interval) => {}
            _ = socket.recv() => bail!("unexpected device message or disconnect"),
        }
        let nonce = protocol::nonce()?;
        timeout(edge.limits.pong_timeout, async {
            send(socket, Message::Ping { nonce }).await?;
            match receive(socket).await? {
                Message::Pong { nonce: response } if response == nonce => Ok(()),
                _ => bail!("invalid heartbeat response"),
            }
        })
        .await??;
    }
}

async fn receive(socket: &mut WebSocket) -> Result<Message> {
    match socket.recv().await {
        Some(Ok(WsMessage::Text(text))) => Ok(serde_json::from_str(&text)?),
        _ => bail!("expected tunnel control message"),
    }
}

async fn send(socket: &mut WebSocket, message: Message) -> Result<()> {
    socket
        .send(WsMessage::Text(serde_json::to_string(&message)?.into()))
        .await?;
    Ok(())
}
