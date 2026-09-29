use std::{collections::HashMap, net::SocketAddr, sync::Arc};

use anyhow::{Context, Result};
use axum::{
    Router,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
};
use tokio::{
    net::TcpListener,
    sync::{RwLock, oneshot},
};

#[derive(Clone, Default)]
pub struct ChallengeResponses(Arc<RwLock<HashMap<String, String>>>);

impl ChallengeResponses {
    pub async fn set(&self, token: String, response: String) {
        self.0.write().await.insert(token, response);
    }

    pub async fn remove(&self, token: &str) {
        self.0.write().await.remove(token);
    }
}

pub struct ChallengeServer {
    responses: ChallengeResponses,
    shutdown: Option<oneshot::Sender<()>>,
    task: Option<tokio::task::JoinHandle<Result<()>>>,
}

impl ChallengeServer {
    pub async fn start(address: SocketAddr) -> Result<Self> {
        let listener = TcpListener::bind(address)
            .await
            .with_context(|| format!("failed to listen for HTTP challenges on {address}"))?;
        let responses = ChallengeResponses::default();
        let router = router(responses.clone());
        let (shutdown, done) = oneshot::channel();
        let task = tokio::spawn(async move {
            axum::serve(listener, router)
                .with_graceful_shutdown(async {
                    let _ = done.await;
                })
                .await
                .context("HTTP challenge server failed")
        });
        Ok(Self {
            responses,
            shutdown: Some(shutdown),
            task: Some(task),
        })
    }

    pub fn responses(&self) -> ChallengeResponses {
        self.responses.clone()
    }

    pub async fn stop(mut self) -> Result<()> {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        self.task
            .take()
            .context("HTTP challenge task is missing")?
            .await
            .context("HTTP challenge task failed")??;
        Ok(())
    }
}

impl Drop for ChallengeServer {
    fn drop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

fn router(responses: ChallengeResponses) -> Router {
    Router::new()
        .route(
            "/.well-known/pontia-edge-address/{nonce}",
            get(address_challenge),
        )
        .route("/.well-known/acme-challenge/{token}", get(acme_challenge))
        .with_state(responses)
}

async fn address_challenge(Path(nonce): Path<String>) -> impl IntoResponse {
    if nonce.len() == 43
        && nonce
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        (StatusCode::OK, nonce)
    } else {
        (StatusCode::NOT_FOUND, String::new())
    }
}

async fn acme_challenge(
    State(responses): State<ChallengeResponses>,
    Path(token): Path<String>,
) -> impl IntoResponse {
    match responses.0.read().await.get(&token).cloned() {
        Some(response) => (StatusCode::OK, response),
        None => (StatusCode::NOT_FOUND, String::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn serves_only_address_and_registered_acme_challenges() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        let server = ChallengeServer::start(address).await.unwrap();
        server
            .responses()
            .set("token".into(), "authorization".into())
            .await;
        let client = reqwest::Client::new();
        let nonce = "A".repeat(43);
        assert_eq!(
            client
                .get(format!(
                    "http://{address}/.well-known/pontia-edge-address/{nonce}"
                ))
                .send()
                .await
                .unwrap()
                .text()
                .await
                .unwrap(),
            nonce
        );
        assert_eq!(
            client
                .get(format!("http://{address}/.well-known/acme-challenge/token"))
                .send()
                .await
                .unwrap()
                .text()
                .await
                .unwrap(),
            "authorization"
        );
        assert_eq!(
            client
                .get(format!("http://{address}/.well-known/acme-challenge/other"))
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::NOT_FOUND
        );
        server.stop().await.unwrap();
    }
}
