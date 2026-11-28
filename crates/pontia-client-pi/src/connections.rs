use pontia_agent_control::{DeliveryError, DeliveryOperation, InputPayload};
use pontia_core::{Error, Result};
use serde_json::{Value, json};
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

pub(crate) type ControlOperation<'a, T = ()> =
    std::pin::Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>;

pub(crate) trait ControlChannel: Send + Sync {
    fn native_history(&self, params: Value) -> ControlOperation<'_, Value>;
    fn compact<'a>(&'a self, operation_id: &'a str) -> ControlOperation<'a>;
    fn set_reasoning_effort<'a>(&'a self, reasoning_effort: &'a str) -> ControlOperation<'a>;
    fn reasoning_effort_control_available(&self) -> bool {
        self.available()
    }
    fn available(&self) -> bool;
    fn process_id(&self) -> Option<u32> {
        None
    }
    fn invalidate(&self);
    fn interrupt(&self) -> DeliveryOperation<'_>;
    fn shutdown(&self) -> ControlOperation<'_>;
    fn shutdown_if_idle(&self) -> ControlOperation<'_, bool>;
    fn replay<'a>(&'a self, replay_id: &'a str) -> ControlOperation<'a>;
    fn start_turn<'a>(&'a self, payload: &'a InputPayload) -> DeliveryOperation<'a>;
    fn steer<'a>(&'a self, payload: &'a InputPayload) -> DeliveryOperation<'a>;
}

struct Connection {
    runtime_id: String,
    channel: Arc<dyn ControlChannel>,
    retired: AtomicBool,
}

#[derive(Default)]
struct Connections {
    entries: HashMap<String, Arc<Connection>>,
    pool: Option<SqlitePool>,
    pontia_home: Option<PathBuf>,
    stopped: bool,
}

#[derive(Clone, Default)]
pub(crate) struct PiConnections {
    connections: Arc<Mutex<Connections>>,
    identity_gate: Arc<Mutex<()>>,
}

impl PiConnections {
    pub(crate) async fn ensure_started(&self, pool: SqlitePool, pontia_home: PathBuf) {
        let mut connections = self.connections.lock().await;
        if connections.pool.is_none() {
            connections.pool = Some(pool);
            connections.pontia_home = Some(pontia_home);
            connections.stopped = false;
        }
    }

    pub(crate) async fn start(&self, pool: SqlitePool, pontia_home: PathBuf) {
        let mut connections = self.connections.lock().await;
        for (_, connection) in connections.entries.drain() {
            connection.retired.store(true, Ordering::SeqCst);
            connection.channel.invalidate();
        }
        connections.pool = Some(pool);
        connections.pontia_home = Some(pontia_home);
        connections.stopped = false;
    }

    pub(crate) async fn lock_identity(&self) -> tokio::sync::OwnedMutexGuard<()> {
        self.identity_gate.clone().lock_owned().await
    }

    async fn current_runtime(pool: &SqlitePool, session_id: &str) -> Result<Option<String>> {
        Ok(sqlx::query_scalar(
            "SELECT r.runtime_id FROM session_runtimes r JOIN sessions s USING(session_id)
             WHERE r.session_id=? AND r.role=?
             AND (s.state IN ('created','starting','idle','busy','waiting_for_interaction','interrupted')
                  OR (s.state='error' AND r.state='starting'))
             AND r.state IN ('starting','running')",
        )
        .bind(session_id)
        .bind(crate::spec::CONTROL_RUNTIME_ROLE)
        .fetch_optional(pool)
        .await?)
    }

    async fn validate_identity(
        pool: &SqlitePool,
        client_session_key: &str,
        session_id: &str,
        runtime_id: &str,
    ) -> Result<()> {
        let binding = pontia_agent_control::AgentBindingService::new(pool.clone())
            .binding_for_client_session("pi", client_session_key)
            .await?
            .ok_or_else(|| Error::NotFound("native binding not found".into()))?;
        if binding.session_id != session_id {
            return Err(Error::StateConflict(
                "native session does not match runtime identity".into(),
            ));
        }
        if Self::current_runtime(pool, session_id).await?.as_deref() != Some(runtime_id) {
            return Err(Error::StateConflict(
                "report runtime is no longer current".into(),
            ));
        }
        Ok(())
    }

    pub(crate) async fn validate_connection(
        &self,
        session_id: &str,
        runtime_id: &str,
        channel: &Arc<dyn ControlChannel>,
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
        let pool = connections.pool.as_ref().ok_or_else(|| {
            Error::CapabilityUnavailable("Pi control service is unavailable".into())
        })?;
        if Self::current_runtime(pool, session_id).await?.as_deref() != Some(runtime_id) {
            return Err(Error::StateConflict(
                "report runtime is no longer current".into(),
            ));
        }
        Ok(())
    }

    pub(crate) async fn attach_locked(
        &self,
        _guard: &tokio::sync::OwnedMutexGuard<()>,
        session_id: &str,
        runtime_id: &str,
        client_session_key: &str,
        channel: Arc<dyn ControlChannel>,
    ) -> Result<()> {
        let mut connections = self.connections.lock().await;
        if connections.stopped || !channel.available() {
            return Err(Error::CapabilityUnavailable(
                "Client connection is closed".into(),
            ));
        }
        let pool = connections.pool.as_ref().ok_or_else(|| {
            Error::CapabilityUnavailable("Pi control service is unavailable".into())
        })?;
        Self::validate_identity(pool, client_session_key, session_id, runtime_id).await?;
        if let Some(pid) = channel.process_id() {
            let confirmed: Option<String> = sqlx::query_scalar(
                "SELECT process_fingerprint FROM session_runtimes WHERE runtime_id=?",
            )
            .bind(runtime_id)
            .fetch_optional(pool)
            .await?
            .flatten();
            let confirmed = confirmed
                .map(|value| serde_json::from_str::<pontia_runtime::ProcessFingerprint>(&value))
                .transpose()?;
            if confirmed.is_none() || confirmed != pontia_runtime::ProcessFingerprint::capture(pid)
            {
                return Err(Error::StateConflict(
                    "Client connection does not belong to the confirmed agent process".into(),
                ));
            }
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
        sqlx::query(
            "UPDATE sessions SET metadata=json_remove(metadata,'$.incompatible_control_runtime')
             WHERE session_id=? AND json_type(metadata,'$.incompatible_control_runtime') IS NOT NULL",
        )
        .bind(session_id)
        .execute(pool)
        .await?;
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
        let Some(connection) = connections.entries.get(session_id) else {
            return Ok(None);
        };
        let pool = connections.pool.as_ref().ok_or_else(|| {
            Error::CapabilityUnavailable("Pi control service is unavailable".into())
        })?;
        let runtime = Self::current_runtime(pool, session_id).await?;
        if Some(connection.runtime_id.as_str()) != runtime.as_deref()
            || !connection.channel.available()
        {
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

    pub(crate) async fn connected_to(&self, session_id: &str, runtime_id: &str) -> Result<bool> {
        Ok(self
            .connection(session_id)
            .await?
            .is_some_and(|connection| connection.runtime_id == runtime_id))
    }

    pub(crate) async fn reasoning_effort_control_available(
        &self,
        session_id: &str,
    ) -> Result<bool> {
        Ok(self
            .connection(session_id)
            .await?
            .is_some_and(|connection| connection.channel.reasoning_effort_control_available()))
    }

    pub(crate) async fn interrupt(
        &self,
        session_id: &str,
        runtime_id: &str,
    ) -> std::result::Result<(), DeliveryError> {
        self.request(session_id, runtime_id, false, |channel| async move {
            channel.interrupt().await
        })
        .await
    }

    pub(crate) async fn shutdown(&self, session_id: &str, runtime_id: &str) -> Result<()> {
        self.request(session_id, runtime_id, true, |channel| async move {
            channel.shutdown().await
        })
        .await
    }

    pub(crate) async fn shutdown_if_idle(
        &self,
        session_id: &str,
        runtime_id: &str,
    ) -> Result<bool> {
        self.request(session_id, runtime_id, true, |channel| async move {
            channel.shutdown_if_idle().await
        })
        .await
    }

    pub(crate) async fn start_turn(
        &self,
        session_id: &str,
        runtime_id: &str,
        payload: &InputPayload,
    ) -> std::result::Result<(), DeliveryError> {
        self.request(session_id, runtime_id, false, |channel| async move {
            channel.start_turn(payload).await
        })
        .await
    }

    pub(crate) async fn replay(
        &self,
        session_id: &str,
        runtime_id: &str,
        replay_id: &str,
    ) -> Result<()> {
        self.request(session_id, runtime_id, false, |channel| async move {
            channel.replay(replay_id).await
        })
        .await
    }

    pub(crate) async fn steer(
        &self,
        session_id: &str,
        runtime_id: &str,
        payload: &InputPayload,
    ) -> std::result::Result<(), DeliveryError> {
        self.request(session_id, runtime_id, false, |channel| async move {
            channel.steer(payload).await
        })
        .await
    }

    pub(crate) async fn compact(
        &self,
        session_id: &str,
        runtime_id: &str,
        operation_id: &str,
    ) -> Result<()> {
        self.request(session_id, runtime_id, false, |channel| async move {
            channel.compact(operation_id).await
        })
        .await
    }

    pub(crate) async fn set_reasoning_effort(
        &self,
        session_id: &str,
        runtime_id: &str,
        reasoning_effort: &str,
    ) -> Result<()> {
        self.request(session_id, runtime_id, false, |channel| async move {
            channel.set_reasoning_effort(reasoning_effort).await
        })
        .await
    }

    async fn request<T, E, F>(
        &self,
        session_id: &str,
        runtime_id: &str,
        expect_exit: bool,
        operation: impl FnOnce(Arc<dyn ControlChannel>) -> F,
    ) -> std::result::Result<T, E>
    where
        E: From<Error> + std::fmt::Display,
        F: Future<Output = std::result::Result<T, E>>,
    {
        let connection = self
            .connection(session_id)
            .await
            .map_err(E::from)?
            .ok_or_else(|| {
                Error::CapabilityUnavailable(format!(
                    "session {session_id} has no current Client connection"
                ))
            })
            .map_err(E::from)?;
        if connection.runtime_id != runtime_id {
            return Err(E::from(Error::StateConflict(
                "Client control runtime is no longer current".into(),
            )));
        }
        let result = operation(connection.channel.clone()).await;
        if let Err(error) = &result {
            self.record_error(session_id, error).await;
        }
        let connections = self.connections.lock().await;
        if connections.stopped || connection.retired.load(Ordering::SeqCst) {
            return Err(E::from(Error::ControlUnknown(
                "Client connection was retired during request".into(),
            )));
        }
        let pool = connections.pool.as_ref().ok_or_else(|| {
            E::from(Error::ControlUnknown(
                "Pi control service is unavailable".into(),
            ))
        })?;
        let current_runtime = Self::current_runtime(pool, session_id)
            .await
            .map_err(|error| E::from(Error::ControlUnknown(error.to_string())))?;
        if current_runtime.as_deref() != Some(runtime_id)
            || !connections
                .entries
                .get(session_id)
                .is_some_and(|current| Arc::ptr_eq(current, &connection))
        {
            if expect_exit
                && result.is_ok()
                && connections
                    .entries
                    .get(session_id)
                    .is_none_or(|current| Arc::ptr_eq(current, &connection))
                && sqlx::query_scalar::<_, bool>(
                    "SELECT EXISTS(SELECT 1 FROM sessions s JOIN session_runtimes r ON r.session_id=s.session_id
                     WHERE s.session_id=? AND s.state='exited' AND r.runtime_id=?)",
                )
                .bind(session_id)
                .bind(runtime_id)
                .fetch_one(pool)
                .await
                .map_err(|error| E::from(Error::ControlUnknown(error.to_string())))?
            {
                return result;
            }
            return Err(E::from(Error::ControlUnknown(
                "Client binding or connection changed during request".into(),
            )));
        }
        result
    }

    pub(crate) async fn refresh_session(&self, session_id: &str) {
        if let Err(error) = self.connection(session_id).await {
            self.record_error(session_id, &error).await;
        }
    }

    async fn record_error(&self, session_id: &str, error: &impl std::fmt::Display) {
        tracing::warn!(session_id, %error, "Pi control channel unavailable");
        let connections = self.connections.lock().await;
        let Some(pontia_home) = &connections.pontia_home else {
            return;
        };
        let path = pontia_runtime::pontia_log_paths(pontia_home).runtime_log;
        let entry = json!({"code":"client_control_unavailable","session_id":session_id,"message":error.to_string()});
        if let Ok(mut log) = pontia_core::private_fs::append_log(&path) {
            let _ = writeln!(log, "{entry}");
        }
    }

    pub(crate) async fn close(&self) {
        let mut connections = self.connections.lock().await;
        connections.stopped = true;
        for (_, connection) in connections.entries.drain() {
            connection.retired.store(true, Ordering::SeqCst);
            connection.channel.invalidate();
        }
    }

    pub(crate) async fn history_source(&self, session_id: &str) -> Result<HistoryConnection> {
        let connection = self.connection(session_id).await?.ok_or_else(|| {
            Error::CapabilityUnavailable(
                "source_unavailable: no current native history connection".into(),
            )
        })?;
        Ok(HistoryConnection {
            connections: self.clone(),
            session_id: session_id.into(),
            connection,
        })
    }
}

#[derive(Clone)]
pub(crate) struct HistoryConnection {
    connections: PiConnections,
    session_id: String,
    connection: Arc<Connection>,
}

impl HistoryConnection {
    pub(crate) async fn validate(&self) -> Result<()> {
        if !self
            .connections
            .connection(&self.session_id)
            .await?
            .is_some_and(|current| {
                Arc::ptr_eq(&current, &self.connection) && !current.retired.load(Ordering::SeqCst)
            })
        {
            return Err(Error::CapabilityUnavailable(
                "source_unavailable: native history connection changed".into(),
            ));
        }
        Ok(())
    }

    pub(crate) async fn read(&self, params: Value) -> Result<Value> {
        self.validate().await?;
        let expected = self.connection.channel.clone();
        let result = self
            .connections
            .request(
                &self.session_id,
                &self.connection.runtime_id,
                false,
                |channel| async move {
                    if !Arc::ptr_eq(&channel, &expected) {
                        return Err(Error::CapabilityUnavailable(
                            "native history connection changed".into(),
                        ));
                    }
                    channel.native_history(params).await
                },
            )
            .await;
        self.validate().await?;
        result.map_err(|_| {
            Error::CapabilityUnavailable("source_unavailable: native history request failed".into())
        })
    }
}
