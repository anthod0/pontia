use std::{
    convert::Infallible,
    error::Error as StdError,
    future::Future,
    io,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll},
    time::Duration,
};

use axum::{
    body::Body,
    extract::ws::{CloseFrame as AxumCloseFrame, Message as AxumMessage, WebSocket},
    http::{Request, Response, StatusCode, Uri},
};
use bytes::Bytes;
use futures_util::{SinkExt, StreamExt, future::BoxFuture};
use http_body::{Body as HttpBody, Frame};
use hyper::{body::Incoming, client::conn::http2::SendRequest, service::service_fn};
use hyper_util::rt::{TokioExecutor, TokioIo, TokioTimer};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, DuplexStream},
    sync::{Mutex, OwnedSemaphorePermit, Semaphore, oneshot, watch},
    task::JoinHandle,
    time::timeout,
};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream,
    tungstenite::{
        Message as TungsteniteMessage,
        protocol::frame::{CloseFrame as TungsteniteCloseFrame, coding::CloseCode},
    },
};

use crate::protocol;

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
const RESPONSE_HEAD_TIMEOUT: Duration = Duration::from_secs(30);
const DRAIN_TIMEOUT: Duration = Duration::from_secs(5);
const REQUEST_BODY_IDLE_TIMEOUT: Duration = Duration::from_secs(30);

type DeviceSocket = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

#[derive(Debug, thiserror::Error)]
pub enum TunnelError {
    #[error("device tunnel is unavailable")]
    Unavailable,
    #[error("device tunnel has no free streams")]
    Overloaded,
    #[error("device tunnel response timed out")]
    Timeout,
    #[error("device tunnel transport failed")]
    Failure,
    #[error("invalid tunneled HTTP request")]
    InvalidRequest,
}

#[derive(Clone)]
pub struct DeviceRequestHandler(
    Arc<dyn Fn(Request<Body>) -> BoxFuture<'static, Response<Body>> + Send + Sync>,
);

impl DeviceRequestHandler {
    pub fn new<F, Fut>(handler: F) -> Self
    where
        F: Fn(Request<Body>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Response<Body>> + Send + 'static,
    {
        Self(Arc::new(move |request| Box::pin(handler(request))))
    }

    async fn handle(&self, request: Request<Body>) -> Response<Body> {
        (self.0)(request).await
    }
}

#[derive(Clone)]
pub struct TunnelConnection {
    sender: Arc<Mutex<SendRequest<Body>>>,
    streams: Arc<Semaphore>,
    unavailable: Arc<AtomicBool>,
}

impl TunnelConnection {
    pub async fn request(&self, request: Request<Body>) -> Result<Response<Body>, TunnelError> {
        if self.unavailable.load(Ordering::Acquire) {
            return Err(TunnelError::Unavailable);
        }
        let (parts, body) = request.into_parts();
        let body_is_empty = body.is_end_stream();
        let invalid_body = Arc::new(AtomicBool::new(false));
        let (body_finished, mut finished) = oneshot::channel();
        let request = Request::from_parts(
            parts,
            Body::new(TunnelRequestBody {
                inner: body,
                finished: if body_is_empty {
                    let _ = body_finished.send(());
                    None
                } else {
                    Some(body_finished)
                },
                invalid: invalid_body.clone(),
            }),
        );
        let permit = self.streams.clone().try_acquire_owned().map_err(|_| {
            if self.unavailable.load(Ordering::Acquire) {
                TunnelError::Unavailable
            } else {
                TunnelError::Overloaded
            }
        })?;
        let response = {
            let mut sender = self.sender.lock().await;
            sender
                .ready()
                .await
                .map_err(|_| self.connection_error(&invalid_body))?;
            sender.send_request(request)
        };
        tokio::pin!(response);
        let response = tokio::select! {
            result = &mut response => result.map_err(|_| self.connection_error(&invalid_body))?,
            completed = &mut finished => {
                if completed.is_err() {
                    response.await.map_err(|_| self.connection_error(&invalid_body))?
                } else {
                    timeout(RESPONSE_HEAD_TIMEOUT, response)
                        .await
                        .map_err(|_| TunnelError::Timeout)?
                        .map_err(|_| self.connection_error(&invalid_body))?
                }
            }
        };
        let (parts, body) = response.into_parts();
        Ok(Response::from_parts(
            parts,
            Body::new(PermitBody {
                inner: body,
                permit: Some(permit),
            }),
        ))
    }

    pub fn mark_unavailable(&self) {
        self.unavailable.store(true, Ordering::Release);
        self.streams.close();
    }

    fn connection_error(&self, invalid_body: &AtomicBool) -> TunnelError {
        if invalid_body.load(Ordering::Acquire) {
            TunnelError::InvalidRequest
        } else if self.unavailable.load(Ordering::Acquire) {
            TunnelError::Unavailable
        } else {
            TunnelError::Failure
        }
    }
}

type BoxError = Box<dyn StdError + Send + Sync>;

struct TunnelRequestBody {
    inner: Body,
    finished: Option<oneshot::Sender<()>>,
    invalid: Arc<AtomicBool>,
}

impl HttpBody for TunnelRequestBody {
    type Data = Bytes;
    type Error = BoxError;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        match Pin::new(&mut self.inner).poll_frame(cx) {
            Poll::Ready(Some(Ok(frame))) if frame.is_trailers() => {
                self.invalid.store(true, Ordering::Release);
                Poll::Ready(Some(Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "HTTP trailers are not supported",
                )
                .into())))
            }
            Poll::Ready(Some(Ok(frame))) => Poll::Ready(Some(Ok(frame))),
            Poll::Ready(Some(Err(error))) => Poll::Ready(Some(Err(error.into()))),
            Poll::Ready(None) => {
                if let Some(finished) = self.finished.take() {
                    let _ = finished.send(());
                }
                Poll::Ready(None)
            }
            Poll::Pending => Poll::Pending,
        }
    }

    fn is_end_stream(&self) -> bool {
        self.inner.is_end_stream()
    }

    fn size_hint(&self) -> http_body::SizeHint {
        http_body::SizeHint::default()
    }
}

struct PermitBody {
    inner: Incoming,
    permit: Option<OwnedSemaphorePermit>,
}

impl HttpBody for PermitBody {
    type Data = Bytes;
    type Error = hyper::Error;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        let result = Pin::new(&mut self.inner).poll_frame(cx);
        if matches!(result, Poll::Ready(None)) || self.inner.is_end_stream() {
            self.permit.take();
        }
        result
    }

    fn is_end_stream(&self) -> bool {
        self.inner.is_end_stream()
    }

    fn size_hint(&self) -> http_body::SizeHint {
        self.inner.size_hint()
    }
}

pub struct TunnelRuntime {
    driver: JoinHandle<Result<(), hyper::Error>>,
    bridge: JoinHandle<io::Result<()>>,
}

impl TunnelRuntime {
    pub async fn run(mut self) -> Result<(), TunnelError> {
        tokio::select! {
            result = &mut self.driver => result.map_err(|_| TunnelError::Failure)?.map_err(|_| TunnelError::Failure),
            result = &mut self.bridge => result.map_err(|_| TunnelError::Failure)?.map_err(|_| TunnelError::Failure),
        }
    }
}

impl Drop for TunnelRuntime {
    fn drop(&mut self) {
        self.driver.abort();
        self.bridge.abort();
    }
}

pub async fn connect_edge(
    socket: WebSocket,
) -> Result<(TunnelConnection, TunnelRuntime), TunnelError> {
    let (io, bridge, peer_ready) = edge_bridge(socket);
    let mut builder = hyper::client::conn::http2::Builder::new(TokioExecutor::new());
    configure_client(&mut builder);
    let handshake = timeout(HANDSHAKE_TIMEOUT, builder.handshake(TokioIo::new(io)))
        .await
        .map_err(|_| TunnelError::Timeout)?
        .map_err(|_| TunnelError::Failure);
    let (mut sender, connection) = match handshake {
        Ok(connection) => connection,
        Err(error) => {
            bridge.abort();
            return Err(error);
        }
    };
    let mut driver = tokio::spawn(connection);
    tokio::select! {
        result = peer_ready => result.map_err(|_| TunnelError::Failure)?,
        _ = &mut driver => {
            bridge.abort();
            return Err(TunnelError::Failure);
        }
        _ = tokio::time::sleep(HANDSHAKE_TIMEOUT) => {
            driver.abort();
            bridge.abort();
            return Err(TunnelError::Timeout);
        }
    }
    let sender_ready = timeout(HANDSHAKE_TIMEOUT, sender.ready()).await;
    if !matches!(sender_ready, Ok(Ok(_))) {
        driver.abort();
        bridge.abort();
        return Err(if sender_ready.is_err() {
            TunnelError::Timeout
        } else {
            TunnelError::Failure
        });
    }
    Ok((
        TunnelConnection {
            sender: Arc::new(Mutex::new(sender)),
            streams: Arc::new(Semaphore::new(protocol::MAX_CONCURRENT_STREAMS)),
            unavailable: Arc::new(AtomicBool::new(false)),
        },
        TunnelRuntime { driver, bridge },
    ))
}

pub async fn serve_device(
    socket: DeviceSocket,
    handler: DeviceRequestHandler,
    mut shutdown: watch::Receiver<bool>,
) -> Result<(), TunnelError> {
    let (io, bridge) = device_bridge(socket);
    let service = service_fn(move |request| {
        let handler = handler.clone();
        async move { Ok::<_, Infallible>(handle_device_request(handler, request).await) }
    });
    let mut builder = hyper::server::conn::http2::Builder::new(TokioExecutor::new());
    configure_server(&mut builder);
    let connection = builder.serve_connection(TokioIo::new(io), service);
    tokio::pin!(connection);
    let result = tokio::select! {
        result = &mut connection => result.map_err(|_| TunnelError::Failure),
        _ = wait_for_shutdown(&mut shutdown) => {
            connection.as_mut().graceful_shutdown();
            timeout(DRAIN_TIMEOUT, &mut connection).await
                .map_err(|_| TunnelError::Timeout)?
                .map_err(|_| TunnelError::Failure)
        }
    };
    bridge.abort();
    result
}

async fn handle_device_request(
    handler: DeviceRequestHandler,
    request: Request<Incoming>,
) -> Response<Body> {
    if !valid_device_request(&request) {
        return invalid_response();
    }
    let (mut parts, body) = request.into_parts();
    let origin = parts
        .uri
        .path_and_query()
        .expect("validated tunnel URI has path")
        .as_str()
        .parse::<Uri>()
        .expect("validated tunnel URI is valid origin-form");
    parts.uri = origin;
    parts.headers = protocol::request_headers(&parts.headers);
    let response = handler
        .handle(Request::from_parts(
            parts,
            Body::new(DeviceRequestBody {
                inner: body,
                idle: Box::pin(tokio::time::sleep(REQUEST_BODY_IDLE_TIMEOUT)),
            }),
        ))
        .await;
    let (mut parts, body) = response.into_parts();
    if parts.status.is_informational() || parts.status == StatusCode::SWITCHING_PROTOCOLS {
        return invalid_response();
    }
    parts.headers = protocol::response_headers(&parts.headers);
    Response::from_parts(parts, Body::new(ResponseBody(body)))
}

struct DeviceRequestBody {
    inner: Incoming,
    idle: Pin<Box<tokio::time::Sleep>>,
}

impl HttpBody for DeviceRequestBody {
    type Data = Bytes;
    type Error = BoxError;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        match Pin::new(&mut self.inner).poll_frame(cx) {
            Poll::Ready(Some(Ok(frame))) if frame.is_trailers() => {
                Poll::Ready(Some(Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "HTTP trailers are not supported",
                )
                .into())))
            }
            Poll::Ready(Some(Ok(frame))) => {
                self.idle
                    .as_mut()
                    .reset(tokio::time::Instant::now() + REQUEST_BODY_IDLE_TIMEOUT);
                Poll::Ready(Some(Ok(frame)))
            }
            Poll::Ready(Some(Err(error))) => Poll::Ready(Some(Err(error.into()))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending if self.idle.as_mut().poll(cx).is_ready() => Poll::Ready(Some(Err(
                io::Error::new(io::ErrorKind::TimedOut, "tunnel request body timed out").into(),
            ))),
            Poll::Pending => Poll::Pending,
        }
    }

    fn is_end_stream(&self) -> bool {
        self.inner.is_end_stream()
    }

    fn size_hint(&self) -> http_body::SizeHint {
        http_body::SizeHint::default()
    }
}

struct ResponseBody(Body);

impl HttpBody for ResponseBody {
    type Data = Bytes;
    type Error = BoxError;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        match Pin::new(&mut self.0).poll_frame(cx) {
            Poll::Ready(Some(Ok(frame))) if frame.is_trailers() => {
                Poll::Ready(Some(Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "HTTP trailers are not supported",
                )
                .into())))
            }
            Poll::Ready(Some(Ok(frame))) => Poll::Ready(Some(Ok(frame))),
            Poll::Ready(Some(Err(error))) => Poll::Ready(Some(Err(error.into()))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }

    fn is_end_stream(&self) -> bool {
        self.0.is_end_stream()
    }

    fn size_hint(&self) -> http_body::SizeHint {
        http_body::SizeHint::default()
    }
}

fn valid_device_request(request: &Request<Incoming>) -> bool {
    protocol::method_allowed(request.method())
        && request.uri().scheme_str() == Some("https")
        && request.uri().authority().map(|value| value.as_str()) == Some("pontia-device")
        && protocol::canonical_api_uri(request.uri())
        && request
            .headers()
            .keys()
            .all(protocol::request_header_allowed)
}

fn invalid_response() -> Response<Body> {
    Response::builder()
        .status(StatusCode::BAD_REQUEST)
        .header("content-type", "application/json")
        .body(Body::from(r#"{"error":{"code":"invalid_tunnel_request","message":"invalid tunneled HTTP request"}}"#))
        .expect("static response")
}

async fn wait_for_shutdown(shutdown: &mut watch::Receiver<bool>) {
    while !*shutdown.borrow_and_update() {
        if shutdown.changed().await.is_err() {
            return;
        }
    }
}

fn configure_client(builder: &mut hyper::client::conn::http2::Builder<TokioExecutor>) {
    builder
        .timer(TokioTimer::new())
        .initial_stream_window_size(protocol::STREAM_WINDOW_BYTES)
        .initial_connection_window_size(protocol::CONNECTION_WINDOW_BYTES)
        .max_frame_size(protocol::MAX_FRAME_BYTES)
        .max_header_list_size(protocol::MAX_HEADER_LIST_BYTES)
        .max_send_buf_size(protocol::MAX_WRITER_BUFFER_BYTES)
        .keep_alive_interval(Some(Duration::from_secs(15)))
        .keep_alive_timeout(Duration::from_secs(10))
        .keep_alive_while_idle(true);
}

fn configure_server(builder: &mut hyper::server::conn::http2::Builder<TokioExecutor>) {
    builder
        .timer(TokioTimer::new())
        .initial_stream_window_size(protocol::STREAM_WINDOW_BYTES)
        .initial_connection_window_size(protocol::CONNECTION_WINDOW_BYTES)
        .max_frame_size(protocol::MAX_FRAME_BYTES)
        .max_header_list_size(protocol::MAX_HEADER_LIST_BYTES)
        .max_concurrent_streams(protocol::MAX_CONCURRENT_STREAMS as u32)
        .max_send_buf_size(protocol::MAX_WRITER_BUFFER_BYTES);
}

fn edge_bridge(
    socket: WebSocket,
) -> (
    DuplexStream,
    JoinHandle<io::Result<()>>,
    oneshot::Receiver<()>,
) {
    let (hyper_io, bridge_io) = tokio::io::duplex(protocol::ADAPTER_BUFFER_BYTES);
    let (ready_tx, ready_rx) = oneshot::channel();
    let task = tokio::spawn(async move {
        let (sink, stream) = socket.split();
        bridge_axum(bridge_io, sink, stream, ready_tx).await
    });
    (hyper_io, task, ready_rx)
}

fn device_bridge(socket: DeviceSocket) -> (DuplexStream, JoinHandle<io::Result<()>>) {
    let (hyper_io, bridge_io) = tokio::io::duplex(protocol::ADAPTER_BUFFER_BYTES);
    let task = tokio::spawn(async move {
        let (sink, stream) = socket.split();
        bridge_tungstenite(bridge_io, sink, stream).await
    });
    (hyper_io, task)
}

async fn bridge_axum(
    io: DuplexStream,
    mut sink: futures_util::stream::SplitSink<WebSocket, AxumMessage>,
    mut stream: futures_util::stream::SplitStream<WebSocket>,
    ready: oneshot::Sender<()>,
) -> io::Result<()> {
    let (mut reader, mut writer) = tokio::io::split(io);
    let protocol_error = {
        let inbound = async {
            let mut ready = Some(ready);
            while let Some(message) = stream.next().await {
                match message.map_err(io::Error::other)? {
                    AxumMessage::Binary(bytes)
                        if bytes.len() <= protocol::MAX_WEBSOCKET_MESSAGE_BYTES =>
                    {
                        if let Some(ready) = ready.take() {
                            let _ = ready.send(());
                        }
                        writer.write_all(&bytes).await?
                    }
                    AxumMessage::Ping(_) | AxumMessage::Pong(_) => {}
                    AxumMessage::Close(_) => return Ok::<bool, io::Error>(false),
                    _ => return Ok::<bool, io::Error>(true),
                }
            }
            Ok(false)
        };
        let outbound = async {
            let mut bytes = vec![0; protocol::MAX_WEBSOCKET_MESSAGE_BYTES];
            loop {
                let count = reader.read(&mut bytes).await?;
                if count == 0 {
                    return Ok(());
                }
                sink.send(AxumMessage::Binary(Bytes::copy_from_slice(&bytes[..count])))
                    .await
                    .map_err(io::Error::other)?;
            }
        };
        tokio::select! {
            result = inbound => result?,
            result = outbound => return result,
        }
    };
    if protocol_error {
        sink.send(AxumMessage::Close(Some(AxumCloseFrame {
            code: 1002,
            reason: "protocol error".into(),
        })))
        .await
        .map_err(io::Error::other)?;
    }
    Ok(())
}

async fn bridge_tungstenite(
    io: DuplexStream,
    mut sink: futures_util::stream::SplitSink<DeviceSocket, TungsteniteMessage>,
    mut stream: futures_util::stream::SplitStream<DeviceSocket>,
) -> io::Result<()> {
    let (mut reader, mut writer) = tokio::io::split(io);
    let protocol_error = {
        let inbound = async {
            while let Some(message) = stream.next().await {
                match message.map_err(io::Error::other)? {
                    TungsteniteMessage::Binary(bytes)
                        if bytes.len() <= protocol::MAX_WEBSOCKET_MESSAGE_BYTES =>
                    {
                        writer.write_all(&bytes).await?
                    }
                    TungsteniteMessage::Ping(_) | TungsteniteMessage::Pong(_) => {}
                    TungsteniteMessage::Close(_) => return Ok::<bool, io::Error>(false),
                    _ => return Ok::<bool, io::Error>(true),
                }
            }
            Ok(false)
        };
        let outbound = async {
            let mut bytes = vec![0; protocol::MAX_WEBSOCKET_MESSAGE_BYTES];
            loop {
                let count = reader.read(&mut bytes).await?;
                if count == 0 {
                    return Ok(());
                }
                sink.send(TungsteniteMessage::Binary(Bytes::copy_from_slice(
                    &bytes[..count],
                )))
                .await
                .map_err(io::Error::other)?;
            }
        };
        tokio::select! {
            result = inbound => result?,
            result = outbound => return result,
        }
    };
    if protocol_error {
        sink.send(TungsteniteMessage::Close(Some(TungsteniteCloseFrame {
            code: CloseCode::Protocol,
            reason: "protocol error".into(),
        })))
        .await
        .map_err(io::Error::other)?;
    }
    Ok(())
}
