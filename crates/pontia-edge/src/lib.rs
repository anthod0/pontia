pub mod acme;
pub mod browser_access;
mod browser_http;
pub mod challenge;
pub mod config;
mod connection;
pub mod credential;
pub mod enrollment;
mod files;
pub mod network;
mod online;
pub mod systemd;
mod tickets;

use std::{sync::Arc, time::Duration};

use axum::{
    Router,
    extract::DefaultBodyLimit,
    routing::{get, post},
};
use tokio::sync::{Semaphore, watch};

pub use browser_access::BrowserAccess;
pub use browser_http::{BOOTSTRAP_PATH, BrowserOrigins, DEVICE_API_PATH};
pub use online::OnlineDevices;
pub use tickets::TicketRedeemer;

#[derive(Clone)]
pub struct ConnectionLimits {
    pub max_pending: usize,
    pub ticket_redeem_timeout: Duration,
    pub heartbeat_interval: Duration,
    pub pong_timeout: Duration,
}

impl Default for ConnectionLimits {
    fn default() -> Self {
        Self {
            max_pending: 256,
            ticket_redeem_timeout: Duration::from_secs(10),
            heartbeat_interval: Duration::from_secs(15),
            pong_timeout: Duration::from_secs(10),
        }
    }
}

#[derive(Clone)]
pub struct Edge {
    pub(crate) redeemer: TicketRedeemer,
    pub(crate) access: BrowserAccess,
    pub(crate) origins: BrowserOrigins,
    online: OnlineDevices,
    pending: Arc<Semaphore>,
    limits: ConnectionLimits,
    shutdown: watch::Sender<bool>,
}

impl Edge {
    pub fn new(
        redeemer: TicketRedeemer,
        access: BrowserAccess,
        origins: BrowserOrigins,
        limits: ConnectionLimits,
    ) -> Self {
        let edge = Self {
            redeemer,
            access,
            origins,
            online: OnlineDevices::default(),
            pending: Arc::new(Semaphore::new(limits.max_pending)),
            limits,
            shutdown: watch::channel(false).0,
        };
        edge.start_capability_cleanup();
        edge
    }

    pub fn online(&self) -> &OnlineDevices {
        &self.online
    }

    pub fn router(&self) -> Router {
        Router::new()
            .route("/healthz", get(|| async { "ok" }))
            .route("/tunnel", get(connection::upgrade))
            .route(
                browser_http::BOOTSTRAP_PATH,
                post(browser_http::bootstrap).layer(DefaultBodyLimit::max(1024)),
            )
            .route(
                browser_http::DEVICE_API_PATH,
                get(browser_http::device_boundary)
                    .post(browser_http::device_boundary)
                    .put(browser_http::device_boundary)
                    .patch(browser_http::device_boundary)
                    .delete(browser_http::device_boundary)
                    .options(browser_http::device_preflight),
            )
            .with_state(self.clone())
    }

    pub fn shutdown(&self) {
        self.pending.close();
        self.shutdown.send_replace(true);
    }

    fn start_capability_cleanup(&self) {
        let access = self.access.clone();
        let mut shutdown = self.shutdown.subscribe();
        tokio::spawn(async move {
            loop {
                if let Err(error) = access.cleanup_expired().await {
                    tracing::error!(%error, "browser capability cleanup failed");
                }
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(60 * 60)) => {}
                    _ = shutdown.wait_for(|stop| *stop) => break,
                }
            }
        });
    }
}
