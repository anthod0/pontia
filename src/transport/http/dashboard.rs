use std::{env, path::PathBuf};

use axum::{
    http::{StatusCode, Uri, header},
    response::{IntoResponse, Response},
};

fn dist_path() -> PathBuf {
    env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("apps/web/dist")
}

pub async fn dashboard() -> Response {
    match tokio::fs::read(dist_path().join("index.html")).await {
        Ok(bytes) => ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], bytes).into_response(),
        Err(_) => (
            StatusCode::NOT_FOUND,
            "Dashboard frontend has not been built. Run `pnpm --dir apps/web build`.",
        )
            .into_response(),
    }
}

pub async fn dashboard_asset(uri: Uri) -> Response {
    let Some(relative_path) = uri.path().strip_prefix("/dashboard/assets/") else {
        return StatusCode::NOT_FOUND.into_response();
    };

    if relative_path.contains("..") || relative_path.starts_with('/') {
        return StatusCode::NOT_FOUND.into_response();
    }

    match tokio::fs::read(dist_path().join("assets").join(relative_path)).await {
        Ok(bytes) => ([(header::CONTENT_TYPE, content_type(relative_path))], bytes).into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

fn content_type(path: &str) -> &'static str {
    if path.ends_with(".js") {
        "text/javascript; charset=utf-8"
    } else if path.ends_with(".css") {
        "text/css; charset=utf-8"
    } else if path.ends_with(".svg") {
        "image/svg+xml"
    } else {
        "application/octet-stream"
    }
}
