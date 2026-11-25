mod headers;
mod loopback;
mod ws;

use axum::{
    body::Body,
    http::{Request, StatusCode, Uri, Version},
    response::{IntoResponse, Response},
};
use hyper::client::conn::http1;
use hyper_util::rt::TokioIo;
use pontia_config::PreviewConfig;
use pontia_tunnel::protocol::{HeaderDirection, TunnelRoute, preview_headers};
use std::time::Duration;
use tokio::{sync::watch, time::timeout};

#[derive(Clone)]
pub struct LocalPreviewProxy {
    bound_port: u16,
    config: PreviewConfig,
    shutdown: watch::Receiver<bool>,
}

impl LocalPreviewProxy {
    pub fn new(bound_port: u16, config: PreviewConfig, shutdown: watch::Receiver<bool>) -> Self {
        Self {
            bound_port,
            config,
            shutdown,
        }
    }

    pub async fn dispatch(
        &self,
        request: Request<Body>,
        e2e: impl AsyncFnOnce(Request<Body>) -> Response,
    ) -> Response {
        let route = TunnelRoute::parse(request.uri());
        let (route, port) = match route {
            Some(TunnelRoute::E2e) => return e2e(request).await,
            Some(route @ (TunnelRoute::PreviewHttp { port } | TunnelRoute::PreviewWs { port })) => {
                (route, port)
            }
            None => return StatusCode::NOT_FOUND.into_response(),
        };
        if !self.config.enabled
            || loopback::forbidden(port, self.bound_port, &self.config.allowed_ports)
        {
            return (
                StatusCode::FORBIDDEN,
                [("x-pontia-preview-error", "port_forbidden")],
            )
                .into_response();
        }
        let Some(host) = headers::preview_host(request.headers()) else {
            return StatusCode::BAD_REQUEST.into_response();
        };
        let (mut parts, body) = request.into_parts();
        let path = parts
            .uri
            .path_and_query()
            .unwrap()
            .as_str()
            .splitn(6, '/')
            .nth(5)
            .expect("parsed preview route");
        let path = format!("/{path}");
        parts.headers = headers::request_headers(&parts.headers, route, port, &host);
        let stream = match loopback::connect(port).await {
            Ok(stream) => stream,
            Err(error) if error.kind() == std::io::ErrorKind::ConnectionRefused => {
                return (
                    StatusCode::BAD_GATEWAY,
                    [("x-pontia-preview-error", "connection_refused")],
                )
                    .into_response();
            }
            Err(error) if error.kind() == std::io::ErrorKind::TimedOut => {
                return StatusCode::GATEWAY_TIMEOUT.into_response();
            }
            Err(_) => return StatusCode::BAD_GATEWAY.into_response(),
        };
        if matches!(route, TunnelRoute::PreviewWs { .. }) {
            return ws::proxy(
                stream,
                port,
                path,
                parts.headers,
                body,
                self.shutdown.clone(),
            )
            .await;
        }
        parts.uri = path.parse::<Uri>().expect("valid origin form");
        parts.version = Version::HTTP_11;
        let Ok((mut sender, connection)) = http1::handshake(TokioIo::new(stream)).await else {
            return StatusCode::BAD_GATEWAY.into_response();
        };
        tokio::spawn(connection);
        let request = Request::from_parts(parts, body);
        let response = match timeout(Duration::from_secs(60), sender.send_request(request)).await {
            Ok(Ok(response)) => response,
            Ok(Err(_)) => return StatusCode::BAD_GATEWAY.into_response(),
            Err(_) => return StatusCode::GATEWAY_TIMEOUT.into_response(),
        };
        let (mut parts, body) = response.into_parts();
        parts.headers = preview_headers(&parts.headers, route, HeaderDirection::Response);
        if parts.status.is_redirection() {
            headers::rewrite_location(&mut parts.headers, port, &host);
        }
        Response::from_parts(parts, Body::new(body))
    }
}
