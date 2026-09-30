use axum::{
    extract::{State, WebSocketUpgrade, ws::WebSocket},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use pontia_tunnel::{connect_edge, protocol};
use tokio::{sync::OwnedSemaphorePermit, time::timeout};
use tracing::{info, warn};
use uuid::Uuid;

use crate::{Edge, tickets::RedeemError};

pub(crate) async fn upgrade(
    State(edge): State<Edge>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    if !offers_protocol(&headers, protocol::SUBPROTOCOL) {
        return StatusCode::BAD_REQUEST.into_response();
    }
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
    ws.protocols([protocol::SUBPROTOCOL])
        .max_message_size(protocol::MAX_WEBSOCKET_MESSAGE_BYTES)
        .max_frame_size(protocol::MAX_WEBSOCKET_MESSAGE_BYTES)
        .write_buffer_size(protocol::ADAPTER_BUFFER_BYTES)
        .max_write_buffer_size(protocol::MAX_WRITER_BUFFER_BYTES)
        .on_upgrade(move |socket| connection(edge, socket, device_id, permit))
}

fn offers_protocol(headers: &HeaderMap, expected: &str) -> bool {
    headers
        .get_all(header::SEC_WEBSOCKET_PROTOCOL)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .any(|value| value.trim() == expected)
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

async fn connection(edge: Edge, socket: WebSocket, device_id: Uuid, permit: OwnedSemaphorePermit) {
    let (handle, runtime) = match connect_edge(socket).await {
        Ok(connection) => connection,
        Err(error) => {
            warn!(%device_id, %error, "device HTTP/2 initialization failed");
            return;
        }
    };
    drop(permit);
    let (_lease, mut replaced) = edge.online.register(device_id, handle.clone());
    info!(%device_id, "device online");
    let mut shutdown = edge.shutdown.subscribe();
    tokio::select! {
        biased;
        _ = shutdown.wait_for(|stop| *stop) => handle.mark_unavailable(),
        _ = replaced.wait_for(|stop| *stop) => handle.mark_unavailable(),
        result = runtime.run() => {
            if let Err(error) = result {
                warn!(%device_id, %error, "device connection closed");
            }
        }
    }
    info!(%device_id, "device disconnected");
}
