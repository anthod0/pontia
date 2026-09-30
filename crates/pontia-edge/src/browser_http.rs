use axum::{
    Json,
    body::Body,
    extract::{Form, OriginalUri, Path, State, rejection::FormRejection},
    http::{HeaderMap, HeaderName, HeaderValue, Method, Request, StatusCode, Uri, header},
    response::{IntoResponse, Response},
};
use pontia_tunnel::{TunnelError, protocol};
use serde::Deserialize;
use serde_json::json;
use time::OffsetDateTime;

use crate::{
    Edge,
    browser_access::{BrowserSecret, COOKIE_NAME, canonical_uuid, parse_canonical_time},
    tickets::RedeemError,
};

pub const BOOTSTRAP_PATH: &str = "/dashboard/bootstrap";
pub const DEVICE_API_PATH: &str = "/devices/{device_id}/api/v1/{*path}";

#[derive(Clone, Debug)]
pub struct BrowserOrigins {
    pub bootstrap: String,
    pub dashboard: String,
}

impl Default for BrowserOrigins {
    fn default() -> Self {
        Self {
            bootstrap: "https://pontia.dev".to_owned(),
            dashboard: "https://app.pontia.dev".to_owned(),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BootstrapForm {
    ticket: String,
}

pub(crate) async fn bootstrap(
    State(edge): State<Edge>,
    headers: HeaderMap,
    OriginalUri(uri): OriginalUri,
    form: Result<Form<BootstrapForm>, FormRejection>,
) -> Response {
    let Ok(Form(form)) = form else {
        return bootstrap_error(StatusCode::BAD_REQUEST);
    };
    if uri.query().is_some()
        || !exact_origin(&headers, &edge.origins.bootstrap)
        || !valid_ticket(&form.ticket)
    {
        return bootstrap_error(StatusCode::BAD_REQUEST);
    }
    let redeemed = match edge.redeemer.redeem_dashboard(&form.ticket).await {
        Ok(redeemed) => redeemed,
        Err(RedeemError::Rejected) => return bootstrap_error(StatusCode::UNAUTHORIZED),
        Err(RedeemError::Unavailable) => {
            return bootstrap_error(StatusCode::SERVICE_UNAVAILABLE);
        }
    };
    let Some(device_id) = canonical_uuid(&redeemed.device_id) else {
        return bootstrap_error(StatusCode::SERVICE_UNAVAILABLE);
    };
    let Some(expires_at) = parse_canonical_time(&redeemed.expires_at) else {
        return bootstrap_error(StatusCode::SERVICE_UNAVAILABLE);
    };
    let now = OffsetDateTime::now_utc();
    if expires_at <= now || !valid_device_handle(&redeemed.device_handle) {
        return bootstrap_error(StatusCode::SERVICE_UNAVAILABLE);
    }
    let secret = match edge.access.reusable_secret(&headers, now).await {
        Some(secret) => secret,
        None => match BrowserSecret::generate() {
            Ok(secret) => secret,
            Err(_) => return bootstrap_error(StatusCode::SERVICE_UNAVAILABLE),
        },
    };
    let cookie_expiry = match edge.access.grant(&secret, device_id, expires_at, now).await {
        Ok(expiry) => expiry,
        Err(error) => {
            tracing::error!(%error, "failed to persist browser capability");
            return bootstrap_error(StatusCode::SERVICE_UNAVAILABLE);
        }
    };
    success_response(
        &secret,
        cookie_expiry,
        now,
        &edge.origins.dashboard,
        &redeemed.device_handle,
    )
}

pub(crate) async fn device_preflight(
    State(edge): State<Edge>,
    Path((_device_id, _path)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    if !exact_origin(&headers, &edge.origins.dashboard) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let requested_method = headers
        .get(header::ACCESS_CONTROL_REQUEST_METHOD)
        .and_then(|value| value.to_str().ok());
    if !matches!(
        requested_method,
        Some("GET" | "POST" | "PUT" | "PATCH" | "DELETE")
    ) {
        return StatusCode::FORBIDDEN.into_response();
    }
    if let Some(requested_headers) = headers
        .get(header::ACCESS_CONTROL_REQUEST_HEADERS)
        .and_then(|value| value.to_str().ok())
        && requested_headers.split(',').map(str::trim).any(|name| {
            !HeaderName::from_bytes(name.as_bytes())
                .is_ok_and(|name| protocol::request_header_allowed(&name))
        })
    {
        return StatusCode::FORBIDDEN.into_response();
    }
    cors_response(StatusCode::NO_CONTENT, &edge.origins.dashboard)
}

pub(crate) async fn device_boundary(
    State(edge): State<Edge>,
    Path((device_id, _path)): Path<(String, String)>,
    OriginalUri(outer_uri): OriginalUri,
    request: Request<Body>,
) -> Response {
    let Some(device_id) = canonical_uuid(&device_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let (parts, body) = request.into_parts();
    let method = parts.method;
    let headers = parts.headers;
    let origin_present = headers.contains_key(header::ORIGIN);
    let valid_origin = exact_origin(&headers, &edge.origins.dashboard);
    if (origin_present || method != Method::GET && method != Method::HEAD) && !valid_origin {
        return StatusCode::FORBIDDEN.into_response();
    }
    if !edge.access.authorize(&headers, device_id).await {
        return cors_if_allowed(
            StatusCode::UNAUTHORIZED,
            valid_origin,
            &edge.origins.dashboard,
        );
    }
    if !protocol::method_allowed(&method) || headers.contains_key(header::TRAILER) {
        return cors_if_allowed(
            StatusCode::BAD_REQUEST,
            valid_origin,
            &edge.origins.dashboard,
        );
    }
    let Some(uri) = tunnel_uri(device_id, &outer_uri) else {
        return cors_if_allowed(
            StatusCode::BAD_REQUEST,
            valid_origin,
            &edge.origins.dashboard,
        );
    };
    let Some(connection) = edge.online.connection(device_id) else {
        return tunnel_error(
            TunnelError::Unavailable,
            valid_origin,
            &edge.origins.dashboard,
        );
    };
    let mut tunnel_request = Request::new(body);
    *tunnel_request.method_mut() = method;
    *tunnel_request.uri_mut() = uri;
    *tunnel_request.headers_mut() = protocol::request_headers(&headers);
    match connection.request(tunnel_request).await {
        Ok(response) => {
            let (mut parts, body) = response.into_parts();
            parts.headers = protocol::response_headers(&parts.headers);
            let mut response = Response::from_parts(parts, body);
            if valid_origin {
                add_cors_headers(response.headers_mut(), &edge.origins.dashboard);
            }
            response
        }
        Err(error) => tunnel_error(error, valid_origin, &edge.origins.dashboard),
    }
}

fn tunnel_uri(device_id: uuid::Uuid, outer: &Uri) -> Option<Uri> {
    let path_and_query = outer.path_and_query()?.as_str();
    let prefix = format!("/devices/{device_id}");
    let origin = path_and_query.strip_prefix(&prefix)?;
    let origin_uri: Uri = origin.parse().ok()?;
    if !protocol::canonical_api_uri(&origin_uri) {
        return None;
    }
    format!("https://pontia-device{origin}").parse().ok()
}

fn tunnel_error(error: TunnelError, cors: bool, origin: &str) -> Response {
    let (status, code, message) = match error {
        TunnelError::Unavailable => (
            StatusCode::SERVICE_UNAVAILABLE,
            "device_unavailable",
            "device is unavailable",
        ),
        TunnelError::Overloaded => (
            StatusCode::SERVICE_UNAVAILABLE,
            "tunnel_overloaded",
            "device tunnel is at capacity",
        ),
        TunnelError::Timeout => (
            StatusCode::GATEWAY_TIMEOUT,
            "tunnel_timeout",
            "device tunnel timed out",
        ),
        TunnelError::Failure => (
            StatusCode::BAD_GATEWAY,
            "tunnel_failure",
            "device tunnel failed",
        ),
        TunnelError::InvalidRequest => (
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "request trailers are not supported",
        ),
    };
    let overloaded = matches!(error, TunnelError::Overloaded);
    let mut response = (
        status,
        Json(json!({
            "data": null,
            "meta": {},
            "error": { "code": code, "message": message }
        })),
    )
        .into_response();
    if overloaded {
        response
            .headers_mut()
            .insert(header::RETRY_AFTER, HeaderValue::from_static("1"));
    }
    if cors {
        add_cors_headers(response.headers_mut(), origin);
    }
    response
}

fn valid_ticket(value: &str) -> bool {
    let Some(encoded) = value.strip_prefix("pet_v1_") else {
        return false;
    };
    if encoded.len() != 43 {
        return false;
    }
    let Ok(decoded) =
        base64::Engine::decode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, encoded)
    else {
        return false;
    };
    decoded.len() == 32
        && base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, decoded)
            == encoded
}

fn valid_device_handle(value: &str) -> bool {
    const RESERVED: &[&str] = &[
        "agent-profiles",
        "api",
        "assets",
        "auth",
        "chat",
        "devices",
        "login",
        "sessions",
        "settings",
        "workflow",
        "workflows",
        "workspace",
        "workspaces",
    ];
    (4..=48).contains(&value.len())
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase())
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_' || byte == b'-'
        })
        && !RESERVED.contains(&value)
}

fn exact_origin(headers: &HeaderMap, expected: &str) -> bool {
    let mut origins = headers.get_all(header::ORIGIN).iter();
    let matches = origins.next().and_then(|value| value.to_str().ok()) == Some(expected);
    matches && origins.next().is_none()
}

fn bootstrap_error(status: StatusCode) -> Response {
    let mut response = (status, "browser access request failed").into_response();
    add_private_headers(response.headers_mut());
    response
}

fn success_response(
    secret: &BrowserSecret,
    expires_at: OffsetDateTime,
    now: OffsetDateTime,
    dashboard_origin: &str,
    handle: &str,
) -> Response {
    let remaining_nanos = (expires_at - now).whole_nanoseconds().max(0);
    let max_age = ceil_seconds(remaining_nanos);
    let cookie_expiry =
        OffsetDateTime::from_unix_timestamp(ceil_seconds(expires_at.unix_timestamp_nanos()) as i64)
            .expect("supported cookie expiry");
    let expires = cookie_expiry
        .format(&time::format_description::parse(
            "[weekday repr:short], [day padding:zero] [month repr:short] [year] [hour]:[minute]:[second] GMT",
        ).expect("valid cookie date format"))
        .expect("supported cookie expiry");
    let cookie = format!(
        "{COOKIE_NAME}={}; Path=/; Expires={expires}; Max-Age={max_age}; Secure; HttpOnly; SameSite=Strict",
        secret.wire()
    );
    let mut response = (
        StatusCode::SEE_OTHER,
        [
            (header::LOCATION, format!("{dashboard_origin}/{handle}")),
            (header::SET_COOKIE, cookie),
        ],
    )
        .into_response();
    add_private_headers(response.headers_mut());
    response
}

fn ceil_seconds(nanoseconds: i128) -> i128 {
    nanoseconds.div_euclid(1_000_000_000) + i128::from(nanoseconds.rem_euclid(1_000_000_000) != 0)
}

fn add_private_headers(headers: &mut HeaderMap) {
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(
        HeaderName::from_static("referrer-policy"),
        HeaderValue::from_static("no-referrer"),
    );
}

fn cors_if_allowed(status: StatusCode, allowed: bool, origin: &str) -> Response {
    if allowed {
        cors_response(status, origin)
    } else {
        status.into_response()
    }
}

fn cors_response(status: StatusCode, origin: &str) -> Response {
    let mut response = status.into_response();
    let headers = response.headers_mut();
    add_cors_headers(headers, origin);
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("GET, POST, PUT, PATCH, DELETE"),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static(protocol::CORS_REQUEST_HEADERS),
    );
    response
}

fn add_cors_headers(headers: &mut HeaderMap, origin: &str) {
    headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin.parse().unwrap());
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_CREDENTIALS,
        HeaderValue::from_static("true"),
    );
    headers.insert(header::VARY, HeaderValue::from_static("Origin"));
}
