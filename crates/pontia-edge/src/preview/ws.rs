use axum::{
    body::Body,
    extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade, close_code},
    http::{HeaderMap, Request, StatusCode, Uri, header},
    response::{IntoResponse, Response},
};
use bytes::{Bytes, BytesMut};
use futures_util::{SinkExt, StreamExt};
use pontia_tunnel::{
    TunnelConnection, TunnelError, WsTunnelFrame,
    ws_frame::{FrameError, MAX_PREVIEW_WS_MESSAGE_BYTES},
};
use std::{convert::Infallible, time::Duration};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use super::Activity;

pub(super) async fn open(
    connection: TunnelConnection,
    uri: Uri,
    headers: HeaderMap,
    mut upgrade: WebSocketUpgrade,
    activity: Option<Activity>,
) -> Result<Response, TunnelError> {
    let (tx, rx) = mpsc::channel(1);
    let mut request = Request::post(uri)
        .body(Body::from_stream(
            ReceiverStream::new(rx).map(Ok::<_, Infallible>),
        ))
        .unwrap();
    *request.headers_mut() = headers;
    let response = connection.open_preview_ws(request).await?;
    if response.status() != StatusCode::OK {
        if response.status().is_informational() {
            return Ok(StatusCode::BAD_GATEWAY.into_response());
        }
        return Ok(response);
    }
    if let Some(selected) = response.headers().get(header::SEC_WEBSOCKET_PROTOCOL) {
        let protocol = selected
            .to_str()
            .map_err(|_| TunnelError::Failure)?
            .to_owned();
        upgrade = upgrade.protocols([protocol]);
        if upgrade.selected_protocol() != Some(selected) {
            return Err(TunnelError::Failure);
        }
    }
    Ok(upgrade
        .max_message_size(MAX_PREVIEW_WS_MESSAGE_BYTES)
        .max_frame_size(MAX_PREVIEW_WS_MESSAGE_BYTES)
        .on_upgrade(move |socket| bridge(socket, tx, response.into_body(), activity)))
}

fn message(frame: WsTunnelFrame) -> Message {
    match frame {
        WsTunnelFrame::Text(s) => Message::Text(s.into()),
        WsTunnelFrame::Binary(bytes) => Message::Binary(bytes),
        WsTunnelFrame::Ping(bytes) => Message::Ping(bytes),
        WsTunnelFrame::Pong(bytes) => Message::Pong(bytes),
        WsTunnelFrame::Close(close) => Message::Close(close.map(|(code, reason)| CloseFrame {
            code,
            reason: reason.into(),
        })),
    }
}

fn frame(msg: Message) -> WsTunnelFrame {
    match msg {
        Message::Text(text) => WsTunnelFrame::Text(text.to_string()),
        Message::Binary(bytes) => WsTunnelFrame::Binary(bytes),
        Message::Ping(bytes) => WsTunnelFrame::Ping(bytes),
        Message::Pong(bytes) => WsTunnelFrame::Pong(bytes),
        Message::Close(close) => {
            WsTunnelFrame::Close(close.map(|c| (c.code, c.reason.to_string())))
        }
    }
}

fn frame_error(error: FrameError) -> u16 {
    match error {
        FrameError::MessageTooBig => close_code::SIZE,
        FrameError::Invalid | FrameError::Truncated => close_code::PROTOCOL,
    }
}

fn browser_error(error: axum::Error) -> u16 {
    match error.into_inner().downcast_ref::<tungstenite::Error>() {
        Some(tungstenite::Error::Capacity(_)) => close_code::SIZE,
        Some(tungstenite::Error::Protocol(_)) => close_code::PROTOCOL,
        _ => close_code::ERROR,
    }
}

async fn bridge(
    socket: WebSocket,
    tx: mpsc::Sender<Bytes>,
    body: Body,
    activity: Option<Activity>,
) {
    let _activity = activity.as_ref().map(Activity::websocket_guard);
    let (mut browser_tx, mut browser_rx) = socket.split();
    let upload = async {
        while let Some(msg) = browser_rx.next().await {
            let frame = frame(msg.map_err(browser_error)?);
            let closing = matches!(frame, WsTunnelFrame::Close(_));
            tx.send(frame.encode().map_err(frame_error)?)
                .await
                .map_err(|_| close_code::AWAY)?;
            if closing {
                return Ok(());
            }
        }
        Err(close_code::AWAY)
    };
    let download = async {
        let mut ping = tokio::time::interval(Duration::from_secs(30));
        ping.tick().await;
        let mut stream = body.into_data_stream();
        let mut buffer = BytesMut::new();
        loop {
            let chunk = tokio::select! {
                _ = ping.tick() => {
                    browser_tx.send(Message::Ping(Bytes::new())).await.map_err(browser_error)?;
                    continue;
                }
                chunk = stream.next() => chunk,
            };
            let Some(chunk) = chunk else {
                WsTunnelFrame::finish(&buffer).map_err(frame_error)?;
                return Err(close_code::AWAY);
            };
            buffer.extend_from_slice(&chunk.map_err(|_| close_code::ERROR)?);
            while let Some(frame) = WsTunnelFrame::decode(&mut buffer).map_err(frame_error)? {
                let closing = matches!(frame, WsTunnelFrame::Close(_));
                browser_tx
                    .send(message(frame))
                    .await
                    .map_err(browser_error)?;
                if closing {
                    return Ok(());
                }
            }
        }
    };
    let result = tokio::select! {
        result = upload => result,
        result = download => result,
    };
    let _ = tokio::time::timeout(Duration::from_secs(2), async {
        if let Err(code) = result {
            let close = WsTunnelFrame::Close(Some((code, String::new())));
            let _ = tokio::join!(
                browser_tx.send(message(close.clone())),
                tx.send(close.encode().unwrap())
            );
        } else {
            let _ = browser_tx.flush().await;
        }
    })
    .await;
}
