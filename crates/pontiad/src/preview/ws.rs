use axum::{
    body::Body,
    http::{HeaderMap, StatusCode, header::SEC_WEBSOCKET_PROTOCOL},
    response::{IntoResponse, Response},
};
use bytes::{Bytes, BytesMut};
use futures_util::{SinkExt, StreamExt};
use pontia_tunnel::{
    WsTunnelFrame,
    ws_frame::{FrameError, MAX_PREVIEW_WS_MESSAGE_BYTES},
};
use std::{convert::Infallible, time::Duration};
use tokio::{
    net::TcpStream,
    sync::{mpsc, watch},
    time::timeout,
};
use tokio_stream::wrappers::ReceiverStream;
use tokio_tungstenite::{
    WebSocketStream, client_async_with_config,
    tungstenite::{
        Error, Message,
        client::IntoClientRequest,
        protocol::{CloseFrame, WebSocketConfig, frame::coding::CloseCode},
    },
};

pub(super) async fn proxy(
    stream: TcpStream,
    port: u16,
    path: String,
    headers: HeaderMap,
    body: Body,
    shutdown: watch::Receiver<bool>,
) -> Response {
    let mut request = format!("ws://localhost:{port}{path}")
        .into_client_request()
        .expect("valid loopback URL");
    request.headers_mut().extend(headers);
    let config = WebSocketConfig::default()
        .max_frame_size(Some(MAX_PREVIEW_WS_MESSAGE_BYTES))
        .max_message_size(Some(MAX_PREVIEW_WS_MESSAGE_BYTES));
    let (socket, handshake) = match timeout(
        Duration::from_secs(15),
        client_async_with_config(request, stream, Some(config)),
    )
    .await
    {
        Ok(Ok(result)) => result,
        Ok(Err(Error::Http(response)))
            if !response.status().is_informational() && response.status() != StatusCode::OK =>
        {
            return response.status().into_response();
        }
        Ok(Err(_)) => return StatusCode::BAD_GATEWAY.into_response(),
        Err(_) => return StatusCode::GATEWAY_TIMEOUT.into_response(),
    };

    let (tx, rx) = mpsc::channel(1);
    tokio::spawn(bridge(socket, body, tx, shutdown));
    let mut response = Response::new(Body::from_stream(
        ReceiverStream::new(rx).map(Ok::<_, Infallible>),
    ));
    if let Some(protocol) = handshake.headers().get(SEC_WEBSOCKET_PROTOCOL) {
        response
            .headers_mut()
            .insert(SEC_WEBSOCKET_PROTOCOL, protocol.clone());
    }
    response
}

fn message(frame: WsTunnelFrame) -> Message {
    match frame {
        WsTunnelFrame::Text(text) => Message::text(text),
        WsTunnelFrame::Binary(bytes) => Message::Binary(bytes),
        WsTunnelFrame::Ping(bytes) => Message::Ping(bytes),
        WsTunnelFrame::Pong(bytes) => Message::Pong(bytes),
        WsTunnelFrame::Close(close) => Message::Close(close.map(|(code, reason)| CloseFrame {
            code: code.into(),
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
            WsTunnelFrame::Close(close.map(|close| (close.code.into(), close.reason.to_string())))
        }
        Message::Frame(_) => unreachable!("tungstenite only reads complete messages"),
    }
}

async fn bridge(
    socket: WebSocketStream<TcpStream>,
    body: Body,
    tx: mpsc::Sender<Bytes>,
    mut shutdown: watch::Receiver<bool>,
) {
    let (mut sink, mut stream) = socket.split();
    let mut input = body.into_data_stream();
    let mut buffer = BytesMut::new();
    let upload = async {
        while let Some(chunk) = input.next().await {
            buffer.extend_from_slice(&chunk.map_err(|_| CloseCode::Error)?);
            while let Some(frame) = WsTunnelFrame::decode(&mut buffer).map_err(frame_error)? {
                let msg = message(frame);
                let closing = msg.is_close();
                sink.send(msg).await.map_err(|_| CloseCode::Error)?;
                if closing {
                    return Ok(());
                }
            }
        }
        WsTunnelFrame::finish(&buffer).map_err(frame_error)?;
        Err(CloseCode::Normal)
    };
    let download = async {
        while let Some(msg) = stream.next().await {
            let msg = msg.map_err(|error| match error {
                Error::Capacity(_) => CloseCode::Size,
                Error::Protocol(_) => CloseCode::Protocol,
                _ => CloseCode::Error,
            })?;
            let closing = msg.is_close();
            tx.send(frame(msg).encode().map_err(frame_error)?)
                .await
                .map_err(|_| CloseCode::Away)?;
            if closing {
                return Ok(());
            }
        }
        Err(CloseCode::Normal)
    };
    tokio::pin!(download);
    let pump = async {
        tokio::select! {
            result = upload => {
                result?;
                timeout(Duration::from_secs(2), &mut download).await.unwrap_or(Err(CloseCode::Away))
            },
            result = &mut download => result,
        }
    };
    let result = tokio::select! {
        result = pump => result,
        _ = shutdown.wait_for(|stop| *stop) => Err(CloseCode::Away),
        _ = tx.closed() => Err(CloseCode::Away),
    };
    let _ = timeout(Duration::from_secs(2), async {
        if let Err(code) = result {
            let close = WsTunnelFrame::Close(Some((code.into(), String::new())));
            let _ = tokio::join!(
                sink.send(message(close.clone())),
                tx.send(close.encode().unwrap())
            );
        } else {
            // Reading a peer Close queues tungstenite's reply.
            let _ = sink.flush().await;
        }
    })
    .await;
}

fn frame_error(error: FrameError) -> CloseCode {
    match error {
        FrameError::MessageTooBig => CloseCode::Size,
        FrameError::Invalid | FrameError::Truncated => CloseCode::Protocol,
    }
}
