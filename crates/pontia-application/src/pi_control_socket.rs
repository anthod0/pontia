use std::{collections::HashMap, io::Write, path::PathBuf, sync::Arc};

use pontia_core::{Error, Result};
use pontia_runtime::{
    pi_control::{PiControlConnection, PiControlEndpoint},
    pontia_log_paths,
};
use pontia_storage_sqlite::repositories::runtime_bindings::SqliteRuntimeBindingRepository;
use serde::Deserialize;
use sqlx::SqlitePool;
use tokio::sync::Mutex;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublishPiControlEndpoint {
    pub session_id: String,
    pub runtime_instance_id: String,
    pub socket_path: String,
    pub version: u32,
}

#[derive(Default)]
struct Connections {
    entries: HashMap<String, Arc<PiControlConnection>>,
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
        self.connection(&request.session_id).await?;
        Ok(())
    }

    async fn connection(&self, session_id: &str) -> Result<Option<Arc<PiControlConnection>>> {
        let mut connections = self.connections.lock().await;
        if connections.stopped {
            return Err(Error::CapabilityUnavailable(
                "Pi control service is stopped".into(),
            ));
        }
        let stored = SqliteRuntimeBindingRepository::new(self.pool.clone())
            .pi_control_endpoint(session_id)
            .await?;
        let endpoint =
            stored.and_then(
                |stored| match serde_json::from_str::<PiControlEndpoint>(&stored) {
                    Ok(endpoint) if endpoint.validate().is_ok() => Some(endpoint),
                    _ => {
                        tracing::warn!(session_id, "invalid persisted Pi control endpoint");
                        None
                    }
                },
            );
        if connections
            .entries
            .get(session_id)
            .is_some_and(|connection| Some(connection.endpoint()) != endpoint.as_ref())
            && let Some(connection) = connections.entries.remove(session_id)
        {
            connection.invalidate();
        }
        let Some(endpoint) = endpoint else {
            return Ok(None);
        };
        if let Some(connection) = connections.entries.get(session_id) {
            return Ok(Some(connection.clone()));
        }
        let connection = Arc::new(PiControlConnection::new(session_id.into(), endpoint)?);
        connections
            .entries
            .insert(session_id.into(), connection.clone());
        Ok(Some(connection))
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
        let connection = self.connection(session_id).await?.ok_or_else(|| {
            Error::CapabilityUnavailable(format!(
                "session {session_id} has no current Pi control endpoint"
            ))
        })?;
        if connection.endpoint().runtime_instance_id != runtime_instance_id {
            return Err(Error::StateConflict(
                "Pi control runtime is no longer current".into(),
            ));
        }
        let result = match submission {
            Some((input, inbox_message_id)) => connection.submit(input, inbox_message_id).await,
            None => connection.ping().await,
        };
        if !self
            .connection(session_id)
            .await?
            .is_some_and(|current| Arc::ptr_eq(&current, &connection))
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

    pub(crate) async fn refresh_session(&self, session_id: &str) {
        if let Err(error) = self.connection(session_id).await {
            self.record_error(session_id, &error);
        }
    }

    pub async fn close(&self) {
        let mut connections = self.connections.lock().await;
        connections.stopped = true;
        for (_, connection) in connections.entries.drain() {
            connection.invalidate();
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
