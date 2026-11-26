use std::path::{Component, Path, PathBuf};

use axum::{
    extract::State,
    http::{HeaderValue, StatusCode, Uri, header},
    response::{IntoResponse, Response},
};

use super::ResolvedDashboard;

pub(crate) async fn security_headers(mut response: Response) -> Response {
    let headers = response.headers_mut();
    headers
        .entry(header::CONTENT_SECURITY_POLICY)
        .or_insert(HeaderValue::from_static(concat!(
            "default-src 'self'; ",
            "script-src 'self' 'unsafe-inline' 'wasm-unsafe-eval'; ",
            "style-src 'self' 'unsafe-inline'; ",
            "img-src 'self' https: data: blob:; ",
            "connect-src 'self' https:; ",
            "frame-src 'self' http: https: blob:; ",
            "form-action 'self' https:; ",
            "object-src 'none'; ",
            "base-uri 'none'; ",
            "frame-ancestors 'none'",
        )));
    headers
        .entry(header::X_FRAME_OPTIONS)
        .or_insert(HeaderValue::from_static("DENY"));
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("strict-origin"),
    );
    headers.insert(
        "permissions-policy",
        HeaderValue::from_static("camera=(), microphone=(), geolocation=(), payment=(), usb=()"),
    );
    response
}

pub async fn dashboard(State(dashboard): State<ResolvedDashboard>) -> Response {
    let Some(root) = dashboard.root() else {
        return (StatusCode::NOT_FOUND, dashboard.unavailable_message()).into_response();
    };

    match tokio::fs::read(root.join("index.html")).await {
        Ok(bytes) => (
            [
                (header::CONTENT_TYPE, "text/html; charset=utf-8"),
                (header::CACHE_CONTROL, "no-cache"),
            ],
            bytes,
        )
            .into_response(),
        Err(_) => (StatusCode::NOT_FOUND, dashboard.unavailable_message()).into_response(),
    }
}

pub async fn dashboard_asset(State(dashboard): State<ResolvedDashboard>, uri: Uri) -> Response {
    let Some(relative_path) = uri.path().strip_prefix("/dashboard/") else {
        return StatusCode::NOT_FOUND.into_response();
    };
    dashboard_dist_file(dashboard, relative_path).await
}

pub async fn dashboard_path(State(dashboard): State<ResolvedDashboard>, uri: Uri) -> Response {
    let Some(relative_path) = uri.path().strip_prefix("/dashboard/") else {
        return StatusCode::NOT_FOUND.into_response();
    };

    match try_dashboard_dist_file(&dashboard, relative_path).await {
        Some(response) => response,
        None => self::dashboard(State(dashboard)).await,
    }
}

async fn dashboard_dist_file(dashboard: ResolvedDashboard, relative_path: &str) -> Response {
    match try_dashboard_dist_file(&dashboard, relative_path).await {
        Some(response) => response,
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn try_dashboard_dist_file(
    dashboard: &ResolvedDashboard,
    relative_path: &str,
) -> Option<Response> {
    let root = dashboard.root()?;
    let safe_path = safe_asset_path(relative_path)?;
    let path = root.join(&safe_path);
    let metadata = tokio::fs::metadata(&path).await.ok()?;
    if !metadata.is_file() {
        return None;
    }

    tokio::fs::read(path).await.ok().map(|bytes| {
        let mut response = (
            [
                (header::CONTENT_TYPE, content_type(relative_path)),
                (header::CACHE_CONTROL, cache_control(relative_path)),
            ],
            bytes,
        )
            .into_response();
        if relative_path == "file-preview.htm" {
            response.headers_mut().insert(
                header::CONTENT_SECURITY_POLICY,
                HeaderValue::from_static("frame-ancestors 'self'; object-src 'none'"),
            );
            response.headers_mut().insert(
                header::X_FRAME_OPTIONS,
                HeaderValue::from_static("SAMEORIGIN"),
            );
        }
        response
    })
}

fn safe_asset_path(relative_path: &str) -> Option<PathBuf> {
    let path = Path::new(relative_path);
    let mut clean = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => clean.push(part),
            _ => return None,
        }
    }
    (!clean.as_os_str().is_empty()).then_some(clean)
}

fn content_type(path: &str) -> &'static str {
    if path == "file-preview.htm" {
        "text/html; charset=utf-8"
    } else if path.ends_with(".js") {
        "text/javascript; charset=utf-8"
    } else if path.ends_with(".css") {
        "text/css; charset=utf-8"
    } else if path.ends_with(".svg") {
        "image/svg+xml"
    } else if path.ends_with(".png") {
        "image/png"
    } else if path.ends_with(".ico") {
        "image/x-icon"
    } else if path.ends_with(".webmanifest") {
        "application/manifest+json"
    } else {
        "application/octet-stream"
    }
}

fn cache_control(path: &str) -> &'static str {
    if path == "service-worker.js" || path.ends_with(".webmanifest") {
        "no-cache"
    } else if path.starts_with("_app/immutable/") {
        "public, max-age=31536000, immutable"
    } else {
        "public, max-age=3600"
    }
}
