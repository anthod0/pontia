use anyhow::{Result, bail};
use axum::{
    extract::{
        State, WebSocketUpgrade,
        ws::{Message as WsMessage, WebSocket},
    },
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use pontia_tunnel::protocol::{self, Message};
use tokio::{sync::OwnedSemaphorePermit, time::timeout};
use tracing::{info, warn};
use uuid::Uuid;

use crate::{Edge, tickets::RedeemError};

pub(crate) async fn upgrade(
    State(edge): State<Edge>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    let permit = match edge.pending.clone().try_acquire_owned() {
        Ok(permit) => permit,
        Err(_) => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    let ticket = match bearer_ticket(&headers) {
        Some(ticket) => ticket,
        None => return unauthorized(),
    };
    let device_id = match timeout(
        edge.limits.ticket_redeem_timeout,
        edge.redeemer.redeem(ticket),
    )
    .await
    {
        Ok(Ok(device_id)) => device_id,
        Ok(Err(RedeemError::Rejected)) => return unauthorized(),
        Ok(Err(RedeemError::Unavailable)) | Err(_) => {
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
    };
    ws.max_message_size(protocol::MAX_MESSAGE_BYTES)
        .max_frame_size(protocol::MAX_MESSAGE_BYTES)
        .on_upgrade(move |socket| connection(edge, socket, device_id, permit))
}

fn bearer_ticket(headers: &HeaderMap) -> Option<&str> {
    let mut values = headers.get_all(header::AUTHORIZATION).iter();
    let value = values.next()?.to_str().ok()?;
    if values.next().is_some() {
        return None;
    }
    let ticket = value.strip_prefix("Bearer ")?;
    if ticket.is_empty() || ticket.bytes().any(|byte| byte.is_ascii_whitespace()) {
        return None;
    }
    Some(ticket)
}

fn unauthorized() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"))],
    )
        .into_response()
}

async fn connection(
    edge: Edge,
    mut socket: WebSocket,
    device_id: Uuid,
    permit: OwnedSemaphorePermit,
) {
    drop(permit);
    let (_lease, mut replaced) = edge.online.register(device_id);
    info!(%device_id, "device online");
    let mut shutdown = edge.shutdown.subscribe();
    tokio::select! {
        biased;
        _ = shutdown.wait_for(|stop| *stop) => {}
        _ = replaced.wait_for(|stop| *stop) => {}
        result = heartbeat(&edge, &mut socket) => {
            if let Err(error) = result {
                warn!(%device_id, %error, "device connection closed");
            }
        }
    }
    info!(%device_id, "device disconnected");
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
