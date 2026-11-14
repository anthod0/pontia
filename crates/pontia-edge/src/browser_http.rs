use axum::{
    Json,
    body::Body,
    extract::{OriginalUri, State},
    http::{HeaderMap, HeaderValue, Method, Request, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use pontia_tunnel::TunnelError;
use serde::Deserialize;

use crate::{Edge, tickets::RedeemError};

pub const SESSIONS_PATH: &str = "/e2e/v1/sessions";
pub const REQUESTS_PATH: &str = "/e2e/v1/requests";
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

pub(crate) async fn cors(State(edge): State<Edge>, request: Request<Body>, next: Next) -> Response {
    if !exact_origin(request.headers(), &edge.origins.dashboard) {
        return error(StatusCode::FORBIDDEN, "invalid_origin");
    }
    let mut response = next.run(request).await;
    add_cors(response.headers_mut(), &edge.origins.dashboard);
    response
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AdmissionRequest {
    ticket: String,
}

pub(crate) async fn admit(
    State(edge): State<Edge>,
    body: Result<Json<AdmissionRequest>, axum::extract::rejection::JsonRejection>,
) -> Response {
    let Ok(Json(body)) = body else {
        return error(StatusCode::BAD_REQUEST, "invalid_request");
    };
    let device_id = match tokio::time::timeout(
        edge.limits.ticket_redeem_timeout,
        edge.redeemer.redeem_admission(&body.ticket),
    )
    .await
    {
        Ok(Ok(device_id)) => device_id,
        Ok(Err(RedeemError::Rejected)) => {
            return error(StatusCode::UNAUTHORIZED, "invalid_admission_ticket");
        }
        _ => return error(StatusCode::SERVICE_UNAVAILABLE, "service_unavailable"),
    };
    let Ok(token) = edge.admissions.issue(device_id) else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "service_unavailable");
    };
    (
        [(header::CACHE_CONTROL, "no-store")],
        Json(serde_json::json!({ "token": token })),
    )
        .into_response()
}

pub(crate) async fn preflight(headers: HeaderMap) -> Response {
    if headers
        .get(header::ACCESS_CONTROL_REQUEST_METHOD)
        .and_then(|value| value.to_str().ok())
        != Some("POST")
    {
        return StatusCode::FORBIDDEN.into_response();
    }
    let mut response = StatusCode::NO_CONTENT.into_response();
    response.headers_mut().insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("POST"),
    );
    response.headers_mut().insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static("content-type, authorization"),
    );
    response
}

pub(crate) async fn relay(
    State(edge): State<Edge>,
    OriginalUri(uri): OriginalUri,
    request: Request<Body>,
) -> Response {
    if request.method() != Method::POST
        || uri.query().is_some()
        || request
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            != Some(CONTENT_TYPE)
        || request.headers().contains_key(header::TRAILER)
    {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let Some(device_id) =
        admission_token(request.headers()).and_then(|token| edge.admissions.authenticate(token))
    else {
        return error(StatusCode::UNAUTHORIZED, "invalid_admission_token");
    };
    let Some(connection) = edge.online.connection(device_id) else {
        return tunnel_error(TunnelError::Unavailable);
    };
    let mut forwarded = Request::new(request.into_body());
    *forwarded.method_mut() = Method::POST;
    *forwarded.uri_mut() = format!("https://pontia-device{}", uri.path())
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
            Response::from_parts(parts, body)
        }
        Err(error) => tunnel_error(error),
    }
}

fn exact_origin(headers: &HeaderMap, expected: &str) -> bool {
    let mut values = headers.get_all(header::ORIGIN).iter();
    values.next().and_then(|value| value.to_str().ok()) == Some(expected) && values.next().is_none()
}

fn admission_token(headers: &HeaderMap) -> Option<&str> {
    let mut values = headers.get_all(header::AUTHORIZATION).iter();
    let token = values.next()?.to_str().ok()?.strip_prefix("Bearer ")?;
    if values.next().is_some()
        || token.is_empty()
        || token.bytes().any(|byte| byte.is_ascii_whitespace())
    {
        return None;
    }
    Some(token)
}

fn error(status: StatusCode, code: &str) -> Response {
    (
        status,
        [(header::CACHE_CONTROL, "no-store")],
        Json(serde_json::json!({ "error": code })),
    )
        .into_response()
}

fn add_cors(headers: &mut HeaderMap, origin: &str) {
    headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin.parse().unwrap());
    headers.insert(header::VARY, HeaderValue::from_static("Origin"));
}

fn tunnel_error(error: TunnelError) -> Response {
    let status = match error {
        TunnelError::Unavailable | TunnelError::Overloaded => StatusCode::SERVICE_UNAVAILABLE,
        TunnelError::Timeout => StatusCode::GATEWAY_TIMEOUT,
        TunnelError::Failure => StatusCode::BAD_GATEWAY,
        TunnelError::InvalidRequest => StatusCode::BAD_REQUEST,
    };
    status.into_response()
}
