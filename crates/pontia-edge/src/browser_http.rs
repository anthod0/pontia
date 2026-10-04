use axum::{
    body::Body,
    extract::{OriginalUri, Path, State},
    http::{HeaderMap, HeaderValue, Method, Request, StatusCode, header},
    response::{IntoResponse, Response},
};
use pontia_tunnel::TunnelError;

use crate::Edge;

pub const SESSIONS_PATH: &str = "/devices/{device_id}/e2e/v1/sessions";
pub const REQUESTS_PATH: &str = "/devices/{device_id}/e2e/v1/requests";
const CONTENT_TYPE: &str = "application/pontia-e2e";

#[derive(Clone, Debug)]
pub struct BrowserOrigins {
    pub dashboard: String,
}

impl Default for BrowserOrigins {
    fn default() -> Self {
        Self {
            dashboard: "https://app.pontia.dev".to_owned(),
        }
    }
}

pub(crate) async fn preflight(State(edge): State<Edge>, headers: HeaderMap) -> Response {
    if !exact_origin(&headers, &edge.origins.dashboard)
        || headers
            .get(header::ACCESS_CONTROL_REQUEST_METHOD)
            .and_then(|value| value.to_str().ok())
            != Some("POST")
    {
        return StatusCode::FORBIDDEN.into_response();
    }
    let mut response = StatusCode::NO_CONTENT.into_response();
    add_cors(response.headers_mut(), &edge.origins.dashboard);
    response.headers_mut().insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("POST"),
    );
    response.headers_mut().insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static("content-type"),
    );
    response
}

pub(crate) async fn relay(
    State(edge): State<Edge>,
    Path(device_id): Path<String>,
    OriginalUri(uri): OriginalUri,
    request: Request<Body>,
) -> Response {
    if request.method() != Method::POST
        || uri.query().is_some()
        || !exact_origin(request.headers(), &edge.origins.dashboard)
        || request
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            != Some(CONTENT_TYPE)
        || request.headers().contains_key(header::TRAILER)
    {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let Ok(parsed_device_id) = uuid::Uuid::parse_str(&device_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if parsed_device_id.get_version_num() != 7 || parsed_device_id.to_string() != device_id {
        return StatusCode::NOT_FOUND.into_response();
    }
    let device_id = parsed_device_id;
    let internal_path = if uri.path().ends_with("/sessions") {
        "/e2e/v1/sessions"
    } else if uri.path().ends_with("/requests") {
        "/e2e/v1/requests"
    } else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(connection) = edge.online.connection(device_id) else {
        return tunnel_error(TunnelError::Unavailable, &edge.origins.dashboard);
    };
    let mut forwarded = Request::new(request.into_body());
    *forwarded.method_mut() = Method::POST;
    *forwarded.uri_mut() = format!("https://pontia-device{internal_path}")
        .parse()
        .unwrap();
    forwarded
        .headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static(CONTENT_TYPE));
    match connection.request(forwarded).await {
        Ok(response) => {
            let (mut parts, body) = response.into_parts();
            let mut headers = HeaderMap::new();
            if let Some(content_type) = parts.headers.get(header::CONTENT_TYPE)
                && content_type == CONTENT_TYPE
            {
                headers.insert(header::CONTENT_TYPE, content_type.clone());
            }
            if let Some(cache_control) = parts.headers.get(header::CACHE_CONTROL) {
                headers.insert(header::CACHE_CONTROL, cache_control.clone());
            }
            parts.headers = headers;
            add_cors(&mut parts.headers, &edge.origins.dashboard);
            Response::from_parts(parts, body)
        }
        Err(error) => tunnel_error(error, &edge.origins.dashboard),
    }
}

fn exact_origin(headers: &HeaderMap, expected: &str) -> bool {
    let mut values = headers.get_all(header::ORIGIN).iter();
    values.next().and_then(|value| value.to_str().ok()) == Some(expected) && values.next().is_none()
}

fn add_cors(headers: &mut HeaderMap, origin: &str) {
    headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin.parse().unwrap());
    headers.insert(header::VARY, HeaderValue::from_static("Origin"));
}

fn tunnel_error(error: TunnelError, origin: &str) -> Response {
    let status = match error {
        TunnelError::Unavailable | TunnelError::Overloaded => StatusCode::SERVICE_UNAVAILABLE,
        TunnelError::Timeout => StatusCode::GATEWAY_TIMEOUT,
        TunnelError::Failure => StatusCode::BAD_GATEWAY,
        TunnelError::InvalidRequest => StatusCode::BAD_REQUEST,
    };
    let mut response = status.into_response();
    add_cors(response.headers_mut(), origin);
    response
}
