use axum::{
    extract::{Form, OriginalUri, Path, State, rejection::FormRejection},
    http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
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
        && requested_headers
            .split(',')
            .map(str::trim)
            .any(|name| !name.eq_ignore_ascii_case("content-type"))
    {
        return StatusCode::FORBIDDEN.into_response();
    }
    cors_response(StatusCode::NO_CONTENT, &edge.origins.dashboard)
}

pub(crate) async fn device_boundary(
    State(edge): State<Edge>,
    Path((device_id, _path)): Path<(String, String)>,
    method: Method,
    headers: HeaderMap,
) -> Response {
    let Some(device_id) = canonical_uuid(&device_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
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
    // The HTTP/SSE tunnel is implemented separately. No request may reach it before this boundary.
    cors_if_allowed(
        StatusCode::NOT_IMPLEMENTED,
        valid_origin,
        &edge.origins.dashboard,
    )
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
    headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin.parse().unwrap());
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_CREDENTIALS,
        HeaderValue::from_static("true"),
    );
    headers.insert(header::VARY, HeaderValue::from_static("Origin"));
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("GET, POST, PUT, PATCH, DELETE"),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static("Content-Type"),
    );
    response
}
