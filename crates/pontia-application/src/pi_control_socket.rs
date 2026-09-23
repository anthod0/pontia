use pontia_core::{Error, Result};
use serde_json::json;
use sqlx::SqlitePool;
use std::{collections::HashMap, future::Future, io::Write, path::PathBuf, pin::Pin, sync::Arc};
use tokio::sync::Mutex;

pub type PiControlOperation<'a, T = ()> = Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>;

pub trait PiControlChannel: Send + Sync {
    fn available(&self) -> bool;
    fn invalidate(&self);
    fn list_models(&self) -> PiControlOperation<'_, Vec<crate::sessions::SessionModel>>;
    fn set_model<'a>(&'a self, model: &'a str) -> PiControlOperation<'a>;
    fn ping(&self) -> PiControlOperation<'_>;
    fn submit<'a>(
        &'a self,
        input: &'a str,
        inbox_message_id: Option<&'a str>,
    ) -> PiControlOperation<'a>;
}

struct Connection {
    runtime_instance_id: String,
    channel: Arc<dyn PiControlChannel>,
}
#[derive(Default)]
struct Connections {
    entries: HashMap<String, Arc<Connection>>,
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

    async fn current_runtime(&self, session_id: &str) -> Result<Option<String>> {
        Ok(sqlx::query_scalar("SELECT r.runtime_instance_id FROM runtime_bindings r JOIN sessions s ON s.session_id=r.session_id WHERE r.session_id=? AND s.client_type='pi' AND s.state IN ('starting','idle','busy','interrupted') AND r.binding_state='confirmed' AND r.runtime_instance_id IS NOT NULL")
            .bind(session_id).fetch_optional(&self.pool).await?)
    }

    pub async fn attach(
        &self,
        session_id: &str,
        runtime_instance_id: &str,
        client_session_key: &str,
        channel: Arc<dyn PiControlChannel>,
    ) -> Result<()> {
        let mut connections = self.connections.lock().await;
        if connections.stopped || !channel.available() {
            return Err(Error::CapabilityUnavailable(
                "Pi connection is closed".into(),
            ));
        }
        let binding = crate::AgentBindingService::new(self.pool.clone())
            .binding_for_client_session("pi", client_session_key)
            .await?
            .ok_or_else(|| Error::NotFound("Pi binding not found".into()))?;
        if binding.session_id != session_id {
            return Err(Error::StateConflict(
                "Pi native session does not match runtime identity".into(),
            ));
        }
        if self.current_runtime(session_id).await?.as_deref() != Some(runtime_instance_id) {
            return Err(Error::StateConflict(
                "Pi connection does not match a current confirmed runtime".into(),
            ));
        }
        if let Some(existing) = connections.entries.get(session_id)
            && existing.runtime_instance_id == runtime_instance_id
            && existing.channel.available()
            && !Arc::ptr_eq(&existing.channel, &channel)
        {
            return Err(Error::StateConflict(
                "Pi runtime already has an active connection".into(),
            ));
        }
        if let Some(previous) = connections.entries.insert(
            session_id.into(),
            Arc::new(Connection {
                runtime_instance_id: runtime_instance_id.into(),
                channel: channel.clone(),
            }),
        ) && !Arc::ptr_eq(&previous.channel, &channel)
        {
            previous.channel.invalidate();
        }
        Ok(())
    }

    async fn connection(&self, session_id: &str) -> Result<Option<Arc<Connection>>> {
        let mut connections = self.connections.lock().await;
        if connections.stopped {
            return Ok(None);
        }
        let runtime = self.current_runtime(session_id).await?;
        if connections
            .entries
            .get(session_id)
            .is_some_and(|connection| {
                Some(connection.runtime_instance_id.as_str()) != runtime.as_deref()
                    || !connection.channel.available()
            })
            && let Some(connection) = connections.entries.remove(session_id)
        {
            connection.channel.invalidate();
        }
        Ok(connections.entries.get(session_id).cloned())
    }

    pub async fn available(&self, session_id: &str) -> Result<bool> {
        Ok(self.connection(session_id).await?.is_some())
    }

    pub async fn ping(&self, session_id: &str, runtime_instance_id: &str) -> Result<()> {
        self.request(session_id, runtime_instance_id, |channel| async move {
            channel.ping().await
        })
        .await
    }

    pub async fn submit(
        &self,
        session_id: &str,
        runtime_instance_id: &str,
        input: &str,
        inbox_message_id: Option<&str>,
    ) -> Result<()> {
        self.request(session_id, runtime_instance_id, |channel| async move {
            channel.submit(input, inbox_message_id).await
        })
        .await
    }

    pub async fn list_models(
        &self,
        session_id: &str,
        runtime_instance_id: &str,
    ) -> Result<Vec<crate::sessions::SessionModel>> {
        self.request(session_id, runtime_instance_id, |channel| async move {
            channel.list_models().await
        })
        .await
    }

    pub async fn set_model(
        &self,
        session_id: &str,
        runtime_instance_id: &str,
        model: &str,
    ) -> Result<()> {
        self.request(session_id, runtime_instance_id, |channel| async move {
            channel.set_model(model).await
        })
        .await
    }

    async fn request<T, F: Future<Output = Result<T>>>(
        &self,
        session_id: &str,
        runtime_instance_id: &str,
        operation: impl FnOnce(Arc<dyn PiControlChannel>) -> F,
    ) -> Result<T> {
        let connection = self.connection(session_id).await?.ok_or_else(|| {
            Error::CapabilityUnavailable(format!(
                "session {session_id} has no current Pi connection"
            ))
        })?;
        if connection.runtime_instance_id != runtime_instance_id {
            return Err(Error::StateConflict(
                "Pi control runtime is no longer current".into(),
            ));
        }
        let result = operation(connection.channel.clone()).await;
        if let Err(error) = &result {
            self.record_error(session_id, error);
        }
        let current_runtime = self
            .current_runtime(session_id)
            .await
            .map_err(|error| Error::ControlUnknown(error.to_string()))?;
        let connections = self.connections.lock().await;
        if current_runtime.as_deref() != Some(runtime_instance_id)
            || !connections
                .entries
                .get(session_id)
                .is_some_and(|current| Arc::ptr_eq(current, &connection))
        {
            return Err(Error::ControlUnknown(
                "Pi binding or connection changed during request".into(),
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
        tracing::warn!(session_id, %error, "Pi control channel unavailable");
        let path = pontia_runtime::pontia_log_paths(&self.pontia_home).runtime_log;
        let entry = json!({"code":"pi_control_unavailable","session_id":session_id,"message":error.to_string()});
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
            connection.channel.invalidate();
        }
    }
}
