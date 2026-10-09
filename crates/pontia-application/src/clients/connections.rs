use pontia_core::{Error, Result};
use serde_json::json;
use sqlx::SqlitePool;
use std::{
    collections::HashMap,
    future::Future,
    io::Write,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio::sync::Mutex;

use crate::client_contract::ClientControlChannel;

struct Connection {
    runtime_id: String,
    channel: Arc<dyn ClientControlChannel>,
    retired: AtomicBool,
}
#[derive(Default)]
struct Connections {
    entries: HashMap<String, Arc<Connection>>,
    stopped: bool,
}

/// Routes only to the current confirmed Client binding; availability never changes lifecycle state.
#[derive(Clone)]
pub struct ClientControlService {
    pool: SqlitePool,
    pontia_home: PathBuf,
    connections: Arc<Mutex<Connections>>,
    identity_gate: Arc<Mutex<()>>,
}

impl ClientControlService {
    pub fn new(pool: SqlitePool, pontia_home: PathBuf) -> Self {
        Self {
            pool,
            pontia_home,
            connections: Arc::default(),
            identity_gate: Arc::default(),
        }
    }

    /// Serializes connection replacement and accepted facts without persisting a generation.
    pub async fn lock_identity(&self) -> tokio::sync::OwnedMutexGuard<()> {
        self.identity_gate.clone().lock_owned().await
    }

    pub async fn validate_connection(
        &self,
        session_id: &str,
        runtime_id: &str,
        channel: &Arc<dyn ClientControlChannel>,
    ) -> Result<()> {
        let connections = self.connections.lock().await;
        if connections.stopped
            || !connections.entries.get(session_id).is_some_and(|current| {
                current.runtime_id == runtime_id
                    && Arc::ptr_eq(&current.channel, channel)
                    && channel.available()
            })
        {
            return Err(Error::StateConflict(
                "Client connection has been replaced".into(),
            ));
        }
        self.validate_runtime(session_id, runtime_id).await
    }

    pub(crate) async fn retire_connection_locked(&self, session_id: &str) {
        if let Some(connection) = self.connections.lock().await.entries.get(session_id) {
            connection.retired.store(true, Ordering::SeqCst);
            connection.channel.invalidate();
        }
    }

    pub async fn validate_runtime(&self, session_id: &str, runtime_id: &str) -> Result<()> {
        let current = pontia_storage_sqlite::repositories::session_runtimes::SqliteSessionRuntimeRepository::new(self.pool.clone())
            .runtime_id(session_id).await?;
        if current.as_deref() != Some(runtime_id) {
            return Err(Error::StateConflict(
                "report runtime is no longer current".into(),
            ));
        }
        Ok(())
    }

    pub async fn validate_identity(
        &self,
        client_type: &str,
        client_session_key: &str,
        session_id: &str,
        runtime_id: &str,
    ) -> Result<()> {
        let binding = crate::AgentBindingService::new(self.pool.clone())
            .binding_for_client_session(client_type, client_session_key)
            .await?
            .ok_or_else(|| Error::NotFound("native binding not found".into()))?;
        if binding.session_id != session_id {
            return Err(Error::StateConflict(
                "native session does not match runtime identity".into(),
            ));
        }
        self.validate_runtime(session_id, runtime_id).await
    }

    async fn current_runtime(&self, session_id: &str) -> Result<Option<String>> {
        let Some(runtime_id) = pontia_storage_sqlite::repositories::session_runtimes::SqliteSessionRuntimeRepository::new(self.pool.clone()).runtime_id(session_id).await? else {
            return Ok(None);
        };
        Ok(sqlx::query_scalar("SELECT r.runtime_id FROM session_runtimes r JOIN sessions s USING(session_id) WHERE r.runtime_id=? AND s.state IN ('starting','idle','busy','interrupted') AND r.state IN ('starting','running')")
            .bind(runtime_id).fetch_optional(&self.pool).await?)
    }

    pub async fn attach(
        &self,
        client_type: &str,
        session_id: &str,
        runtime_id: &str,
        client_session_key: &str,
        channel: Arc<dyn ClientControlChannel>,
    ) -> Result<()> {
        let guard = self.lock_identity().await;
        self.attach_locked(
            &guard,
            client_type,
            session_id,
            runtime_id,
            client_session_key,
            channel,
        )
        .await
    }

    pub async fn attach_locked(
        &self,
        _guard: &tokio::sync::OwnedMutexGuard<()>,
        client_type: &str,
        session_id: &str,
        runtime_id: &str,
        client_session_key: &str,
        channel: Arc<dyn ClientControlChannel>,
    ) -> Result<()> {
        let mut connections = self.connections.lock().await;
        if connections.stopped || !channel.available() {
            return Err(Error::CapabilityUnavailable(
                "Client connection is closed".into(),
            ));
        }
        self.validate_identity(client_type, client_session_key, session_id, runtime_id)
            .await?;
        if let Some(pid) = channel.process_id()
            && let Some(record) = pontia_storage_sqlite::repositories::session_runtimes::SqliteSessionRuntimeRepository::new(self.pool.clone()).get(runtime_id).await?
            && let Some(fingerprint) = record.process_fingerprint.as_deref().and_then(|json| serde_json::from_str::<pontia_runtime::TmuxProcessFingerprint>(json).ok())
            && (fingerprint.agent_pid != pid || pontia_runtime::GenericRuntimeManager.observe_tmux_process_fingerprint(&fingerprint) == pontia_runtime::ProcessObservation::Exited) {
            return Err(Error::StateConflict("Client connection does not belong to the confirmed agent process".into()));
        }
        if self.current_runtime(session_id).await?.as_deref() != Some(runtime_id) {
            return Err(Error::StateConflict(
                "Client connection does not match a current confirmed runtime".into(),
            ));
        }
        if let Some(existing) = connections.entries.get(session_id)
            && existing.runtime_id == runtime_id
            && Arc::ptr_eq(&existing.channel, &channel)
            && !existing.retired.load(Ordering::SeqCst)
        {
            return Ok(());
        }
        if let Some(existing) = connections.entries.get(session_id)
            && existing.runtime_id == runtime_id
            && existing.channel.available()
            && !Arc::ptr_eq(&existing.channel, &channel)
        {
            return Err(Error::StateConflict(
                "Client runtime already has an active connection".into(),
            ));
        }
        if let Some(previous) = connections.entries.insert(
            session_id.into(),
            Arc::new(Connection {
                runtime_id: runtime_id.into(),
                channel: channel.clone(),
                retired: AtomicBool::new(false),
            }),
        ) && !Arc::ptr_eq(&previous.channel, &channel)
        {
            previous.retired.store(true, Ordering::SeqCst);
            previous.channel.invalidate();
        }
        Ok(())
    }

    async fn connection(&self, session_id: &str) -> Result<Option<Arc<Connection>>> {
        let connections = self.connections.lock().await;
        if connections.stopped {
            return Ok(None);
        }
        let runtime = self.current_runtime(session_id).await?;
        let Some(connection) = connections.entries.get(session_id) else {
            return Ok(None);
        };
        if Some(connection.runtime_id.as_str()) != runtime.as_deref()
            || !connection.channel.available()
        {
            // Retain the last connection's provenance through its own exit so a
            // replacement can retire pending receipts even after availability ends.
            if runtime
                .as_deref()
                .is_some_and(|id| id != connection.runtime_id)
            {
                connection.retired.store(true, Ordering::SeqCst);
            }
            connection.channel.invalidate();
            return Ok(None);
        }
        Ok(Some(connection.clone()))
    }

    pub async fn available(&self, session_id: &str) -> Result<bool> {
        Ok(self.connection(session_id).await?.is_some())
    }

    pub async fn interrupt(&self, session_id: &str, runtime_id: &str) -> Result<()> {
        self.request(session_id, runtime_id, false, |channel| async move {
            channel.interrupt().await
        })
        .await
    }

    pub async fn shutdown(&self, session_id: &str, runtime_id: &str) -> Result<()> {
        self.request(session_id, runtime_id, true, |channel| async move {
            channel.shutdown().await
        })
        .await
    }

    pub async fn ping(&self, session_id: &str, runtime_id: &str) -> Result<()> {
        self.request(session_id, runtime_id, false, |channel| async move {
            channel.ping().await
        })
        .await
    }

    pub async fn submit(
        &self,
        session_id: &str,
        runtime_id: &str,
        input: &str,
        inbox_message_id: Option<&str>,
    ) -> Result<()> {
        self.request(session_id, runtime_id, false, |channel| async move {
            channel.submit(input, inbox_message_id).await
        })
        .await
    }

    pub async fn replay(
        &self,
        session_id: &str,
        runtime_id: &str,
        inbox_message_id: &str,
    ) -> Result<()> {
        self.request(session_id, runtime_id, false, |channel| async move {
            channel.replay(inbox_message_id).await
        })
        .await
    }

    pub async fn list_models(
        &self,
        session_id: &str,
        runtime_id: &str,
    ) -> Result<Vec<crate::sessions::SessionModel>> {
        self.request(session_id, runtime_id, false, |channel| async move {
            channel.list_models().await
        })
        .await
    }

    pub async fn set_model(&self, session_id: &str, runtime_id: &str, model: &str) -> Result<()> {
        self.request(session_id, runtime_id, false, |channel| async move {
            channel.set_model(model).await
        })
        .await
    }

    async fn request<T, F: Future<Output = Result<T>>>(
        &self,
        session_id: &str,
        runtime_id: &str,
        expect_exit: bool,
        operation: impl FnOnce(Arc<dyn ClientControlChannel>) -> F,
    ) -> Result<T> {
        let connection = self.connection(session_id).await?.ok_or_else(|| {
            Error::CapabilityUnavailable(format!(
                "session {session_id} has no current Client connection"
            ))
        })?;
        if connection.runtime_id != runtime_id {
            return Err(Error::StateConflict(
                "Client control runtime is no longer current".into(),
            ));
        }
        let result = operation(connection.channel.clone()).await;
        if let Err(error) = &result {
            self.record_error(session_id, error);
        }
        let connections = self.connections.lock().await;
        if connections.stopped || connection.retired.load(Ordering::SeqCst) {
            return Err(Error::ControlUnknown(
                "Client connection was retired during request".into(),
            ));
        }
        let current_runtime = self
            .current_runtime(session_id)
            .await
            .map_err(|error| Error::ControlUnknown(error.to_string()))?;
        if current_runtime.as_deref() != Some(runtime_id)
            || !connections
                .entries
                .get(session_id)
                .is_some_and(|current| Arc::ptr_eq(current, &connection))
        {
            // A shutdown acknowledgement may race with its own session.exited fact.
            // Accept that exit only after a valid reply, never after a lost reply or replacement.
            if expect_exit
                && result.is_ok()
                && connections.entries.get(session_id)
                    .is_none_or(|current| Arc::ptr_eq(current, &connection))
                && sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM sessions s JOIN session_runtimes r ON r.session_id=s.session_id WHERE s.session_id=? AND s.state='exited' AND r.runtime_id=?)")
                    .bind(session_id).bind(runtime_id).fetch_one(&self.pool).await
                    .map_err(|error| Error::ControlUnknown(error.to_string()))?
            {
                return result;
            }
            return Err(Error::ControlUnknown(
                "Client binding or connection changed during request".into(),
            ));
        }
        result
    }

    pub(crate) async fn refresh_session(&self, session_id: &str) {
        if let Err(error) = self.connection(session_id).await {
            self.record_error(session_id, &error);
        }
    }

    fn record_error(&self, session_id: &str, error: &Error) {
        tracing::warn!(session_id, %error, "Client control channel unavailable");
        let path = pontia_runtime::pontia_log_paths(&self.pontia_home).runtime_log;
        let entry = json!({"code":"client_control_unavailable","session_id":session_id,"message":error.to_string()});
        if let Ok(mut log) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let _ = writeln!(log, "{entry}");
        }
    }

    pub async fn close(&self) {
        let mut connections = self.connections.lock().await;
        connections.stopped = true;
        for (_, connection) in connections.entries.drain() {
            connection.retired.store(true, Ordering::SeqCst);
            connection.channel.invalidate();
        }
    }
}

#[cfg(test)]
mod tests;
