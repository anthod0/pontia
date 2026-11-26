use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use axum_server::{Handle, tls_rustls::RustlsConfig};
use serde::Serialize;
use tokio::{sync::Mutex, task::JoinHandle};
use uuid::Uuid;

use crate::{
    Edge,
    auth::bearer_token,
    config::PortPreviewConfig,
    network::bind_listener,
    preview::{Activity, Target},
};

const OFFLINE_GRACE: u64 = 5 * 60;
const REAP_INTERVAL: Duration = Duration::from_secs(60);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct BindingKey {
    device: Uuid,
    target_port: u16,
}

struct ActiveListener {
    key: BindingKey,
    listen_port: u16,
    handle: Handle<SocketAddr>,
    task: JoinHandle<std::io::Result<()>>,
    max_exp_unix: Arc<AtomicU64>,
    activity: Activity,
    offline_since_unix: u64,
}

#[derive(Default)]
struct Bindings {
    by_key: HashMap<BindingKey, u16>,
    by_port: HashMap<u16, ActiveListener>,
}

#[derive(Clone)]
pub(crate) struct PreviewPortManager(Arc<Manager>);

struct Manager {
    edge: Edge,
    hostname: String,
    allowed_ports: Vec<u16>,
    max_listeners: usize,
    tls: RustlsConfig,
    bindings: Mutex<Bindings>,
}

#[derive(Serialize)]
struct OpenedPreview {
    preview_origin: String,
    listen_port: u16,
    target_port: u16,
    remapped: bool,
}

impl PreviewPortManager {
    pub(crate) fn new(
        edge: Edge,
        hostname: &str,
        control_port: u16,
        config: &PortPreviewConfig,
        tls: RustlsConfig,
    ) -> Self {
        let manager = Self(Arc::new(Manager {
            edge,
            hostname: hostname.to_owned(),
            allowed_ports: config
                .allowed_ports(control_port)
                .expect("validated preview port configuration"),
            max_listeners: config.max_listeners,
            tls,
            bindings: Mutex::new(Bindings::default()),
        }));
        tokio::spawn(manager.clone().reap());
        manager
    }

    pub(crate) async fn open(State(manager): State<Self>, headers: HeaderMap) -> Response {
        let Some(verified) = bearer_token(&headers)
            .and_then(|token| manager.0.edge.access_tokens.preview.verify_any(token))
        else {
            return api_error(StatusCode::UNAUTHORIZED, "invalid_preview_token");
        };
        if manager.0.edge.online.connection(verified.device).is_none() {
            return api_error(StatusCode::SERVICE_UNAVAILABLE, "device_offline");
        }
        let key = BindingKey {
            device: verified.device,
            target_port: verified.port,
        };
        match manager.allocate(key, verified.remaining_secs).await {
            Some(opened) => ([(header::CACHE_CONTROL, "no-store")], Json(opened)).into_response(),
            None => api_error(StatusCode::SERVICE_UNAVAILABLE, "preview_ports_exhausted"),
        }
    }

    pub(crate) async fn unavailable() -> Response {
        api_error(StatusCode::CONFLICT, "preview_unavailable")
    }

    async fn allocate(&self, key: BindingKey, remaining_secs: u64) -> Option<OpenedPreview> {
        let now = unix_time();
        let mut bindings = self.0.bindings.lock().await;
        if let Some(port) = bindings.by_key.get(&key).copied() {
            let active = bindings.by_port.get(&port).expect("binding indexes agree");
            active
                .max_exp_unix
                .fetch_max(now.saturating_add(remaining_secs), Ordering::Relaxed);
            active.activity.touch();
            return Some(self.opened(port, key.target_port));
        }

        if bindings.by_port.len() >= self.0.max_listeners {
            let evict = bindings
                .by_port
                .values()
                .filter(|active| active.activity.websockets() == 0)
                .min_by_key(|active| active.activity.last_activity())
                .map(|active| active.listen_port)?;
            Self::remove(&mut bindings, evict).await;
        }

        let mut candidates = Vec::with_capacity(self.0.allowed_ports.len());
        if self.0.allowed_ports.binary_search(&key.target_port).is_ok() {
            candidates.push(key.target_port);
        }
        candidates.extend(
            self.0
                .allowed_ports
                .iter()
                .copied()
                .filter(|port| *port != key.target_port),
        );
        for listen_port in candidates {
            if bindings.by_port.contains_key(&listen_port) {
                continue;
            }
            let listener = match bind_listener(listen_port) {
                Ok(listener) => listener,
                Err(error) => {
                    tracing::debug!(listen_port, %error, "preview port unavailable");
                    continue;
                }
            };
            let handle = Handle::new();
            let activity = Activity::new();
            let max_exp_unix = Arc::new(AtomicU64::new(now.saturating_add(remaining_secs)));
            let router = super::port_router(
                self.0.edge.clone(),
                &self.0.hostname,
                listen_port,
                Target {
                    device: key.device,
                    port: key.target_port,
                },
                activity.clone(),
            );
            let task = tokio::spawn(
                axum_server::from_tcp_rustls(listener, self.0.tls.clone())
                    .expect("bound preview listener is usable")
                    .handle(handle.clone())
                    .serve(router.into_make_service()),
            );
            bindings.by_key.insert(key, listen_port);
            bindings.by_port.insert(
                listen_port,
                ActiveListener {
                    key,
                    listen_port,
                    handle,
                    task,
                    max_exp_unix,
                    activity,
                    offline_since_unix: 0,
                },
            );
            return Some(self.opened(listen_port, key.target_port));
        }
        None
    }

    fn opened(&self, listen_port: u16, target_port: u16) -> OpenedPreview {
        let host = match self.0.hostname.parse::<IpAddr>() {
            Ok(IpAddr::V6(_)) => format!("[{}]", self.0.hostname),
            _ => self.0.hostname.clone(),
        };
        OpenedPreview {
            preview_origin: format!("https://{host}:{listen_port}"),
            listen_port,
            target_port,
            remapped: listen_port != target_port,
        }
    }

    async fn remove(bindings: &mut Bindings, port: u16) {
        let Some(active) = bindings.by_port.remove(&port) else {
            return;
        };
        bindings.by_key.remove(&active.key);
        active
            .handle
            .graceful_shutdown(Some(Duration::from_secs(5)));
        let _ = tokio::time::timeout(Duration::from_secs(6), active.task).await;
    }

    async fn reap(self) {
        let mut shutdown = self.0.edge.shutdown.subscribe();
        let mut interval = tokio::time::interval(REAP_INTERVAL);
        interval.tick().await;
        loop {
            tokio::select! {
                _ = interval.tick() => self.reap_once().await,
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() {
                        let mut bindings = self.0.bindings.lock().await;
                        let ports: Vec<_> = bindings.by_port.keys().copied().collect();
                        for port in ports {
                            Self::remove(&mut bindings, port).await;
                        }
                        return;
                    }
                }
            }
        }
    }

    async fn reap_once(&self) {
        let now = unix_time();
        let mut bindings = self.0.bindings.lock().await;
        for active in bindings.by_port.values_mut() {
            if self.0.edge.online.connection(active.key.device).is_some() {
                active.offline_since_unix = 0;
            } else if active.offline_since_unix == 0 {
                active.offline_since_unix = now;
            }
        }
        let ports: Vec<_> = bindings
            .by_port
            .values()
            .filter(|active| should_reap(active, now))
            .map(|active| active.listen_port)
            .collect();
        for port in ports {
            Self::remove(&mut bindings, port).await;
        }
    }
}

fn should_reap(active: &ActiveListener, now: u64) -> bool {
    (active.activity.websockets() == 0 && now > active.max_exp_unix.load(Ordering::Relaxed))
        || (active.offline_since_unix != 0
            && now.saturating_sub(active.offline_since_unix) >= OFFLINE_GRACE)
}

fn unix_time() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn api_error(status: StatusCode, code: &str) -> Response {
    (
        status,
        [(header::CACHE_CONTROL, "no-store")],
        Json(serde_json::json!({ "error": code })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listener(max_exp_unix: u64, offline_since_unix: u64, activity: Activity) -> ActiveListener {
        ActiveListener {
            key: BindingKey {
                device: Uuid::now_v7(),
                target_port: 5173,
            },
            listen_port: 20_000,
            handle: Handle::new(),
            task: tokio::spawn(async { Ok(()) }),
            max_exp_unix: Arc::new(AtomicU64::new(max_exp_unix)),
            activity,
            offline_since_unix,
        }
    }

    #[tokio::test]
    async fn reaping_follows_token_expiry_offline_grace_and_active_websockets() {
        let now = 10_000;
        let activity = Activity::new();
        let expired = listener(now - 1, 0, activity.clone());
        assert!(should_reap(&expired, now));

        let _websocket = activity.websocket_guard();
        assert!(!should_reap(&expired, now));

        let recently_offline = listener(now + 1, now - OFFLINE_GRACE + 1, Activity::new());
        assert!(!should_reap(&recently_offline, now));
        let offline = listener(now + 1, now - OFFLINE_GRACE, Activity::new());
        assert!(should_reap(&offline, now));
    }
}
