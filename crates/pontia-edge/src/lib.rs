mod connection;
mod devices;
mod online;

use std::{sync::Arc, time::Duration};

use axum::{Router, routing::get};
use tokio::sync::{Semaphore, watch};

pub use devices::DeviceRegistry;
pub use online::OnlineDevices;

#[derive(Clone)]
pub struct ConnectionLimits {
    pub max_pending: usize,
    pub auth_timeout: Duration,
    pub heartbeat_interval: Duration,
    pub pong_timeout: Duration,
}

impl Default for ConnectionLimits {
    fn default() -> Self {
        Self {
            max_pending: 256,
            auth_timeout: Duration::from_secs(10),
            heartbeat_interval: Duration::from_secs(15),
            pong_timeout: Duration::from_secs(10),
        }
    }
}

#[derive(Clone)]
pub struct Edge {
    devices: DeviceRegistry,
    online: OnlineDevices,
    pending: Arc<Semaphore>,
    limits: ConnectionLimits,
    shutdown: watch::Sender<bool>,
}

impl Edge {
    pub fn new(devices: DeviceRegistry, limits: ConnectionLimits) -> Self {
        Self {
            devices,
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
