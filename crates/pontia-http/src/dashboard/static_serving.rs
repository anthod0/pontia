use std::path::{Component, Path, PathBuf};

use axum::{
    extract::State,
    http::{StatusCode, Uri, header},
    response::{IntoResponse, Response},
};

use super::ResolvedDashboard;

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
        (
            [
                (header::CONTENT_TYPE, content_type(relative_path)),
                (header::CACHE_CONTROL, cache_control(relative_path)),
            ],
            bytes,
        )
            .into_response()
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
    if path.ends_with(".js") {
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
