mod connection;
pub mod credential;
mod online;
mod tickets;

use std::{sync::Arc, time::Duration};

use axum::{Router, routing::get};
use tokio::sync::{Semaphore, watch};

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
    redeemer: TicketRedeemer,
    online: OnlineDevices,
    pending: Arc<Semaphore>,
    limits: ConnectionLimits,
    shutdown: watch::Sender<bool>,
}

impl Edge {
    pub fn new(redeemer: TicketRedeemer, limits: ConnectionLimits) -> Self {
        Self {
            redeemer,
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
            .with_state(self.clone())
    }

    pub fn shutdown(&self) {
        self.pending.close();
        self.shutdown.send_replace(true);
    }
}
