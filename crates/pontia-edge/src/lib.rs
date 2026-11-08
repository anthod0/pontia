pub mod acme;
mod acme_dns;
mod admissions;
mod browser_http;
pub mod challenge;
pub mod config;
mod connection;
pub mod credential;
pub mod enrollment;
mod files;
pub mod network;
mod online;
pub mod pending_init;
pub mod port;
pub mod systemd;
mod tickets;
pub mod tls;

use std::{sync::Arc, time::Duration};

use axum::{
    Router,
    extract::DefaultBodyLimit,
    middleware,
    routing::{get, post},
};
use tokio::sync::{Semaphore, watch};

pub use browser_http::{BrowserOrigins, REQUESTS_PATH, SESSIONS_PATH};
pub use online::OnlineDevices;
pub use tickets::TicketRedeemer;

#[derive(Clone)]
pub struct ConnectionLimits {
    pub max_pending: usize,
    pub ticket_redeem_timeout: Duration,
}

impl Default for ConnectionLimits {
    fn default() -> Self {
        Self {
            max_pending: 256,
            ticket_redeem_timeout: Duration::from_secs(10),
        }
    }
}

#[derive(Clone)]
pub struct Edge {
    pub(crate) redeemer: TicketRedeemer,
    pub(crate) origins: BrowserOrigins,
    admissions: admissions::Admissions,
    online: OnlineDevices,
    pending: Arc<Semaphore>,
    limits: ConnectionLimits,
    shutdown: watch::Sender<bool>,
}

impl Edge {
    pub fn new(
        redeemer: TicketRedeemer,
        origins: BrowserOrigins,
        limits: ConnectionLimits,
    ) -> Self {
        Self {
            redeemer,
            origins,
            admissions: admissions::Admissions::default(),
            online: OnlineDevices::default(),
            pending: Arc::new(Semaphore::new(limits.max_pending)),
            limits,
            shutdown: watch::channel(false).0,
        }
    }

    pub fn online(&self) -> &OnlineDevices {
        &self.online
    }

    pub fn router(&self) -> Router {
        Router::new()
            .route("/healthz", get(|| async { "ok" }))
            .route("/tunnel", get(connection::upgrade))
            .merge(
                Router::new()
                    .route(
                        "/browser-admissions",
                        post(browser_http::admit).options(browser_http::preflight),
                    )
                    .route(
                        browser_http::SESSIONS_PATH,
                        post(browser_http::relay).options(browser_http::preflight),
                    )
                    .route(
                        browser_http::REQUESTS_PATH,
                        post(browser_http::relay).options(browser_http::preflight),
                    )
                    .route_layer(middleware::from_fn_with_state(
                        self.clone(),
                        browser_http::cors,
                    )),
            )
            .layer(DefaultBodyLimit::max(2 * 1024 * 1024))
            .with_state(self.clone())
    }

    pub fn shutdown(&self) {
        self.pending.close();
        self.shutdown.send_replace(true);
    }
}
