use std::{collections::HashMap, sync::Arc};

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

use crate::network::bind_listener;

#[derive(Clone, Default)]
pub struct ChallengeResponses(Arc<RwLock<HashMap<String, String>>>);

impl ChallengeResponses {
    pub async fn set(&self, token: String, response: String) {
        self.0.write().await.insert(token, response);
    }

    pub(crate) async fn is_active(&self) -> bool {
        !self.0.read().await.is_empty()
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
    pub async fn start_acme(port: u16) -> Result<Self> {
        Self::serve(bind_listener(port)?)
            .with_context(|| format!("failed to listen for HTTP challenges on port {port}"))
    }

    fn serve(listener: std::net::TcpListener) -> Result<Self> {
        let listener = TcpListener::from_std(listener)?;
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

pub(crate) fn router(responses: ChallengeResponses) -> Router {
    Router::new()
        .route("/.well-known/acme-challenge/{token}", get(acme_challenge))
        .with_state(responses)
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
    async fn serves_only_registered_acme_challenges() {
        let listener = bind_listener(0).unwrap();
        let address =
            std::net::SocketAddr::from(([127, 0, 0, 1], listener.local_addr().unwrap().port()));
        let server = ChallengeServer::serve(listener).unwrap();
        server
            .responses()
            .set("token".into(), "authorization".into())
            .await;
        let client = reqwest::Client::builder()
            .pool_max_idle_per_host(0)
            .build()
            .unwrap();
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
        for path in [
            "/.well-known/acme-challenge/other",
            &format!("/.well-known/pontia-edge-address/{}", "A".repeat(43)),
        ] {
            assert_eq!(
                client
                    .get(format!("http://{address}{path}"))
                    .send()
                    .await
                    .unwrap()
                    .status(),
                StatusCode::NOT_FOUND
            );
        }
        server.stop().await.unwrap();
        // The challenge listener fully releases the port before TLS starts on it.
        drop(bind_listener(address.port()).unwrap());
    }
}
