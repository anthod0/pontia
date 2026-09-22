use std::{collections::HashMap, io::Write, path::PathBuf, sync::Arc, time::Duration};

use pontia_core::{Error, Result};
use pontia_runtime::{
    pi_control::{PiControlConnection, PiControlEndpoint},
    pontia_log_paths,
};
use pontia_storage_sqlite::repositories::runtime_bindings::SqliteRuntimeBindingRepository;
use serde::Deserialize;
use sqlx::SqlitePool;
use tokio::{
    sync::{Mutex, watch},
    time::Instant,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublishPiControlEndpoint {
    pub session_id: String,
    pub runtime_instance_id: String,
    pub socket_path: String,
    pub version: u32,
}

struct ManagedConnection {
    connection: Arc<PiControlConnection>,
    next_attempt: Option<Instant>,
    retry_delay: Duration,
}

#[derive(Default)]
struct Connections {
    entries: HashMap<String, ManagedConnection>,
    stopped: bool,
}

/// Routes only to the current confirmed Pi binding; availability never changes lifecycle state.
#[derive(Clone)]
pub struct PiControlService {
    pool: SqlitePool,
    pontia_home: PathBuf,
    connections: Arc<Mutex<Connections>>,
}

impl PiControlService {
    pub fn new(pool: SqlitePool, pontia_home: PathBuf) -> Self {
        Self {
            pool,
            pontia_home,
            connections: Arc::default(),
        }
    }

    pub async fn publish_endpoint(&self, request: PublishPiControlEndpoint) -> Result<()> {
        let endpoint = PiControlEndpoint {
            runtime_instance_id: request.runtime_instance_id,
            socket_path: request.socket_path,
            version: request.version,
        };
        endpoint.validate()?;
        SqliteRuntimeBindingRepository::new(self.pool.clone())
            .publish_pi_control_endpoint(
                &request.session_id,
                &endpoint.runtime_instance_id,
                &serde_json::to_string(&endpoint)?,
            )
            .await?;
        self.reconcile().await
    }

    async fn reconcile(&self) -> Result<()> {
        let mut connections = self.connections.lock().await;
        if connections.stopped {
            return Err(Error::CapabilityUnavailable(
                "Pi control service is stopped".into(),
            ));
        }
        let rows = SqliteRuntimeBindingRepository::new(self.pool.clone())
            .pi_control_bindings()
            .await?;
        let mut current = HashMap::new();
        for row in rows {
            match serde_json::from_str::<PiControlEndpoint>(&row.endpoint) {
                Ok(endpoint) if endpoint.validate().is_ok() => {
                    current.insert(row.session_id, endpoint);
                }
                _ => tracing::warn!(
                    session_id = row.session_id,
                    "invalid persisted Pi control endpoint"
                ),
            }
        }
        connections.entries.retain(|session_id, managed| {
            let keep = current.get(session_id) == Some(managed.connection.endpoint());
            if !keep {
                managed.connection.invalidate();
            }
            keep
        });
        for (session_id, endpoint) in current {
            if let std::collections::hash_map::Entry::Vacant(entry) =
                connections.entries.entry(session_id)
            {
                let connection = Arc::new(PiControlConnection::new(entry.key().clone(), endpoint)?);
                entry.insert(ManagedConnection {
                    connection,
                    next_attempt: Some(Instant::now()),
                    retry_delay: Duration::from_secs(1),
                });
            }
        }
        Ok(())
    }

    pub async fn ping(&self, session_id: &str, runtime_instance_id: &str) -> Result<()> {
        self.request(session_id, runtime_instance_id, None).await
    }

    pub async fn submit(
        &self,
        session_id: &str,
        runtime_instance_id: &str,
        input: &str,
        inbox_message_id: Option<&str>,
    ) -> Result<()> {
        self.request(
            session_id,
            runtime_instance_id,
            Some((input, inbox_message_id)),
        )
        .await
    }

    async fn request(
        &self,
        session_id: &str,
        runtime_instance_id: &str,
        submission: Option<(&str, Option<&str>)>,
    ) -> Result<()> {
        self.reconcile().await?;
        let connection = {
            let connections = self.connections.lock().await;
            let managed = connections.entries.get(session_id).ok_or_else(|| {
                Error::CapabilityUnavailable(format!(
                    "session {session_id} has no current Pi control endpoint"
                ))
            })?;
            if managed.connection.endpoint().runtime_instance_id != runtime_instance_id {
                return Err(Error::StateConflict(
                    "Pi control runtime is no longer current".into(),
                ));
            }
            managed.connection.clone()
        };
        let result = match submission {
            Some((input, inbox_message_id)) => connection.submit(input, inbox_message_id).await,
            None => connection.ping().await,
        };
        self.reconcile().await?;
        let connections = self.connections.lock().await;
        if !connections
            .entries
            .get(session_id)
            .is_some_and(|entry| Arc::ptr_eq(&entry.connection, &connection))
        {
            return Err(Error::StateConflict(
                "Pi control binding changed during request".into(),
            ));
        }
        if let Err(error) = &result {
            self.record_error(session_id, error);
        }
        result
    }

    pub async fn run(&self, mut shutdown: watch::Receiver<bool>) {
        let mut tick = tokio::time::interval(Duration::from_secs(1));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut requests = tokio::task::JoinSet::new();
        loop {
            if *shutdown.borrow() {
                break;
            }
            tokio::select! {
                _ = shutdown.changed() => break,
                Some(result) = requests.join_next(), if !requests.is_empty() => {
                    if let Ok((session_id, connection, success)) = result {
                        let mut connections = self.connections.lock().await;
                        if let Some(entry) = connections.entries.get_mut(&session_id)
                            && Arc::ptr_eq(&entry.connection, &connection) {
                            let delay = if success { Duration::from_secs(10) } else { entry.retry_delay };
                            entry.next_attempt = Some(Instant::now() + delay);
                            entry.retry_delay = if success { Duration::from_secs(1) } else { (entry.retry_delay * 2).min(Duration::from_secs(30)) };
                        }
                    }
                }
                _ = tick.tick() => {
                    if let Err(error) = self.reconcile().await {
                        self.record_error("", &error);
                        continue;
                    }
                    let mut connections = self.connections.lock().await;
                    for (session_id, entry) in &mut connections.entries {
                        if !entry.next_attempt.is_some_and(|deadline| deadline <= Instant::now()) { continue; }
                        // One background probe per entry; completion sets the next deadline.
                        entry.next_attempt = None;
                        let service = self.clone();
                        let session_id = session_id.clone();
                        let connection = entry.connection.clone();
                        requests.spawn(async move {
                            let success = service.ping(&session_id, &connection.endpoint().runtime_instance_id).await.is_ok();
                            (session_id, connection, success)
                        });
                    }
                }
            }
        }
        self.close().await;
        requests.shutdown().await;
    }

    pub async fn close(&self) {
        let mut connections = self.connections.lock().await;
        connections.stopped = true;
        for (_, entry) in connections.entries.drain() {
            entry.connection.invalidate();
        }
    }

    fn record_error(&self, session_id: &str, error: &Error) {
        tracing::warn!(session_id, %error, "Pi control channel unavailable");
        let path = pontia_log_paths(&self.pontia_home).runtime_log;
        let entry = serde_json::json!({
            "code": "pi_control_unavailable", "session_id": session_id, "message": error.to_string(),
        });
        if let Ok(mut log) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let _ = writeln!(log, "{entry}");
        }
    }
}
