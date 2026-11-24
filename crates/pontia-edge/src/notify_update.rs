use std::{
    path::Path,
    sync::{Arc, Mutex},
};

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
};
use uuid::Uuid;

use crate::{Edge, browser_http::bearer_token, systemd::Systemd};

#[derive(Clone)]
pub(crate) struct Notifications(Arc<Mutex<Uuid>>);

impl Default for Notifications {
    fn default() -> Self {
        Self(Arc::new(Mutex::new(Uuid::new_v4())))
    }
}

pub(crate) async fn health(
    State(edge): State<Edge>,
) -> ([(&'static str, String); 3], &'static str) {
    let nonce = *edge.updates.0.lock().unwrap();
    (
        [
            ("pontia-edge-version", pontia_version::version().to_string()),
            ("pontia-update-nonce", nonce.to_string()),
            ("cache-control", "no-store".into()),
        ],
        "ok",
    )
}

pub(crate) async fn notify(State(edge): State<Edge>, headers: HeaderMap) -> StatusCode {
    let Some(nonce) =
        bearer_token(&headers).and_then(|token| edge.access_tokens.update_nonce(token))
    else {
        return StatusCode::UNAUTHORIZED;
    };
    {
        let mut current = edge.updates.0.lock().unwrap();
        if *current != nonce {
            return StatusCode::CONFLICT;
        }
        *current = Uuid::new_v4();
    }
    if !Path::new("/run/systemd/system").is_dir() {
        return StatusCode::NO_CONTENT;
    }
    tokio::spawn(async {
        if let Err(error) = Systemd::default().request_auto_update().await {
            tracing::warn!(%error, "could not queue automatic Edge update");
        }
    });
    StatusCode::ACCEPTED
}
