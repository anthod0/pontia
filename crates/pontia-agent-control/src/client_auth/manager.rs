use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{Arc, Mutex, MutexGuard, Once, Weak},
    time::Duration,
};

use pontia_core::{Error, Result};
use pontia_storage_sqlite::repositories::agent_clients::SqliteAgentClientRepository;
use sqlx::SqlitePool;
use tokio::{
    sync::{broadcast, mpsc, watch},
    task::JoinSet,
    time::Instant,
};

use super::sanitize::sanitize_output;
use crate::{
    agent_clients::AgentClientService,
    client_contract::{
        ClientAuth,
        auth::{
            AuthCancel, AuthContext, AuthErrorCode, AuthEventSink, AuthFlowUpdate, AuthInput,
            AuthSessionSnapshot, AuthSessionState, AuthState, AuthStep, ClientAuthStatus,
            StartLoginParams, rfc3339,
        },
    },
    clients::ClientRegistry,
    context::ShutdownSignal,
};

const START_TIMEOUT: Duration = Duration::from_secs(30);
const CANCEL_TIMEOUT: Duration = Duration::from_secs(10);
const SHUTDOWN_CANCEL_TIMEOUT: Duration = Duration::from_secs(3);
const LOGOUT_TIMEOUT: Duration = Duration::from_secs(15);
const INPUT_TIMEOUT: Duration = Duration::from_secs(5);
const STATUS_RETRY_DELAY: Duration = Duration::from_millis(300);
const EVENT_CAPACITY: usize = 64;
const MIN_PROVIDER_TTL_SECS: u64 = 60;
const MAX_PROVIDER_TTL_SECS: u64 = 900;

#[derive(Clone, Copy)]
pub(super) struct Limits {
    pub active_ttl: Duration,
    pub terminal_retention: Duration,
    pub reaper_interval: Duration,
    pub max_sessions: usize,
    pub status_timeout: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            active_ttl: Duration::from_secs(600),
            terminal_retention: Duration::from_secs(300),
            reaper_interval: Duration::from_secs(15),
            max_sessions: 16,
            status_timeout: Duration::from_secs(5),
        }
    }
}

type ClientId = (String, String);

#[derive(Default)]
struct Table {
    sessions: HashMap<String, Entry>,
    /// The non-terminal login session of each client entry.
    active: HashMap<ClientId, String>,
    logouts: HashSet<ClientId>,
}

struct Entry {
    snapshot: AuthSessionSnapshot,
    deadline: Instant,
    evict_at: Option<Instant>,
    input: Option<mpsc::Sender<AuthInput>>,
    cancel: Option<AuthCancel>,
    /// The client returned its flow handle.
    started: bool,
    events: broadcast::Sender<AuthSessionSnapshot>,
}

impl Entry {
    fn owned_by(&self, client: &str, key: &str) -> bool {
        self.snapshot.client_type == client && self.snapshot.client_key == key
    }

    fn update(&mut self, state: AuthSessionState, step: AuthStep) {
        let now = pontia_core::time::utc_now();
        self.snapshot.state = state;
        self.snapshot.step = step;
        self.snapshot.sequence += 1;
        self.snapshot.updated_at = rfc3339(now);
        if state.is_terminal() {
            self.snapshot.terminal_at = Some(self.snapshot.updated_at.clone());
        }
        let _ = self.events.send(self.snapshot.clone());
    }
}

/// Settled sessions keep their snapshot for reconnects until `evict_at`.
fn settle(
    table: &mut Table,
    id: &str,
    state: AuthSessionState,
    step: AuthStep,
    retention: Duration,
) -> Option<(AuthSessionSnapshot, Option<AuthCancel>)> {
    let entry = table
        .sessions
        .get_mut(id)
        .filter(|entry| !entry.snapshot.state.is_terminal())?;
    entry.update(state, step);
    entry.evict_at = Some(Instant::now() + retention);
    entry.input = None;
    let identity = (
        entry.snapshot.client_type.clone(),
        entry.snapshot.client_key.clone(),
    );
    let result = (entry.snapshot.clone(), entry.cancel.take());
    if table
        .active
        .get(&identity)
        .is_some_and(|active| active == id)
    {
        table.active.remove(&identity);
    }
    Some(result)
}

fn conflict(code: &'static str, message: impl Into<String>) -> Error {
    Error::Conflict {
        code,
        message: message.into(),
    }
}

fn not_found() -> Error {
    Error::NotFound("Login session not found".into())
}

async fn run_cancel(cancel: AuthCancel, limit: Duration) {
    match tokio::time::timeout(limit, cancel()).await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            tracing::warn!(error = %sanitize_output(&error.to_string()), "Cannot cancel login flow");
        }
        Err(_) => tracing::warn!("Login flow cancellation timed out"),
    }
}

/// Owns every in-progress Agent Client login session on this device.
#[derive(Clone)]
pub struct ClientAuthService {
    inner: Arc<Inner>,
}

struct Inner {
    table: Mutex<Table>,
    clients: ClientRegistry,
    db: SqlitePool,
    pontia_home: PathBuf,
    shutdown: ShutdownSignal,
    limits: Limits,
    reaper: Once,
}

impl ClientAuthService {
    pub(crate) fn new(
        db: SqlitePool,
        clients: ClientRegistry,
        pontia_home: PathBuf,
        shutdown: ShutdownSignal,
    ) -> Self {
        Self::with_limits(db, clients, pontia_home, shutdown, Limits::default())
    }

    pub(super) fn with_limits(
        db: SqlitePool,
        clients: ClientRegistry,
        pontia_home: PathBuf,
        shutdown: ShutdownSignal,
        limits: Limits,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                table: Mutex::default(),
                clients,
                db,
                pontia_home,
                shutdown,
                limits,
                reaper: Once::new(),
            }),
        }
    }

    fn table(&self) -> MutexGuard<'_, Table> {
        self.inner.table.lock().expect("login session table")
    }

    fn context(&self) -> AuthContext {
        AuthContext {
            pontia_home: self.inner.pontia_home.clone(),
            vars: Arc::new(std::env::vars().collect()),
        }
    }

    /// The registered authentication of a known client entry, if Pontia manages it.
    async fn auth(&self, client: &str, key: &str) -> Result<Option<Arc<dyn ClientAuth>>> {
        let registration = self
            .inner
            .clients
            .get(client)
            .ok_or_else(|| Error::NotFound(format!("Client not found: {client}")))?;
        let configured = registration
            .configuration
            .as_ref()
            .is_some_and(|configuration| configuration.keys().iter().any(|known| known == key));
        if !configured
            && SqliteAgentClientRepository::new(self.inner.db.clone())
                .get(client, key)
                .await?
                .is_none()
        {
            return Err(Error::NotFound("Client entry not found".into()));
        }
        Ok(registration
            .auth
            .clone()
            .filter(|auth| auth.supports_auth(key)))
    }

    async fn managed_auth(&self, client: &str, key: &str) -> Result<Arc<dyn ClientAuth>> {
        self.auth(client, key).await?.ok_or_else(|| {
            Error::CapabilityUnavailable(format!(
                "Pontia does not manage authentication for {client}/{key}"
            ))
        })
    }

    async fn probe(&self, auth: &dyn ClientAuth, key: &str) -> ClientAuthStatus {
        let context = self.context();
        let unknown = |detail: String| {
            ClientAuthStatus::new(AuthState::Unknown, Vec::new(), false).with_detail(detail)
        };
        match tokio::time::timeout(self.inner.limits.status_timeout, auth.status(key, &context))
            .await
        {
            Ok(Ok(mut status)) => {
                status.detail = status.detail.as_deref().map(sanitize_output);
                status
            }
            Ok(Err(error)) => unknown(sanitize_output(&error.to_string())),
            Err(_) => unknown("Authentication status check timed out".into()),
        }
    }

    fn active_session(&self, client: &str, key: &str) -> Option<String> {
        self.table()
            .active
            .get(&(client.to_owned(), key.to_owned()))
            .cloned()
    }

    pub async fn status(&self, client: &str, key: &str) -> Result<ClientAuthStatus> {
        let mut status = match self.auth(client, key).await? {
            Some(auth) => self.probe(auth.as_ref(), key).await,
            None => ClientAuthStatus::new(AuthState::Unsupported, Vec::new(), false),
        };
        status.active_session_id = self.active_session(client, key);
        Ok(status)
    }

    pub async fn start_login(
        &self,
        client: &str,
        key: &str,
        method_id: &str,
        params: StartLoginParams,
    ) -> Result<AuthSessionSnapshot> {
        let auth = self.managed_auth(client, key).await?;
        self.ensure_reaper();
        let id = format!("auth_{}", uuid::Uuid::now_v7());
        let identity = (client.to_owned(), key.to_owned());
        {
            let mut table = self.table();
            if table.logouts.contains(&identity) {
                return Err(conflict(
                    "auth_operation_in_progress",
                    "Logout is in progress for this client",
                ));
            }
            if let Some(active) = table.active.get(&identity) {
                return Err(conflict(
                    "auth_session_in_progress",
                    format!("Login session {active} is already in progress for this client"),
                ));
            }
            if table.active.len() >= self.inner.limits.max_sessions {
                return Err(conflict(
                    "auth_capacity_exceeded",
                    "Too many login sessions are in progress",
                ));
            }
            let now = pontia_core::time::utc_now();
            let created_at = rfc3339(now);
            let ttl = self.inner.limits.active_ttl;
            table.sessions.insert(
                id.clone(),
                Entry {
                    snapshot: AuthSessionSnapshot {
                        session_id: id.clone(),
                        client_type: client.into(),
                        client_key: key.into(),
                        method_id: method_id.into(),
                        state: AuthSessionState::Starting,
                        step: AuthStep::Pending {
                            message: Some("Starting login".into()),
                        },
                        sequence: 1,
                        updated_at: created_at.clone(),
                        created_at,
                        expires_at: rfc3339(now + ttl),
                        terminal_at: None,
                    },
                    deadline: Instant::now() + ttl,
                    evict_at: None,
                    input: None,
                    cancel: None,
                    started: false,
                    events: broadcast::channel(EVENT_CAPACITY).0,
                },
            );
            table.active.insert(identity, id.clone());
        }
        let (sender, updates) = mpsc::channel(16);
        tokio::spawn(self.clone().pump(id.clone(), updates));
        let context = self.context();
        let started = tokio::time::timeout(
            START_TIMEOUT,
            auth.start_login(key, method_id, params, &context, AuthEventSink::new(sender)),
        )
        .await
        .unwrap_or_else(|_| {
            Err(Error::CapabilityUnavailable(
                "Login did not start in time".into(),
            ))
        });
        let handle = match started {
            Ok(handle) => handle,
            Err(error) => {
                self.rollback(&id);
                return Err(error);
            }
        };
        let abandoned = {
            let mut table = self.table();
            match table.sessions.get_mut(&id) {
                Some(entry) if !entry.snapshot.state.is_terminal() => {
                    entry.input = handle.input;
                    entry.cancel = Some(handle.cancel);
                    entry.started = true;
                    None
                }
                // Cancelled or timed out while the client was starting.
                Some(entry)
                    if matches!(
                        entry.snapshot.state,
                        AuthSessionState::Cancelled | AuthSessionState::TimedOut
                    ) =>
                {
                    Some(handle.cancel)
                }
                _ => None,
            }
        };
        if let Some(cancel) = abandoned {
            run_cancel(cancel, CANCEL_TIMEOUT).await;
        }
        self.table()
            .sessions
            .get(&id)
            .map(|entry| entry.snapshot.clone())
            .ok_or_else(not_found)
    }

    fn rollback(&self, id: &str) {
        let mut table = self.table();
        if table
            .sessions
            .get(id)
            .is_some_and(|entry| !entry.snapshot.state.is_terminal())
            && let Some(entry) = table.sessions.remove(id)
        {
            let identity = (entry.snapshot.client_type, entry.snapshot.client_key);
            if table
                .active
                .get(&identity)
                .is_some_and(|active| active == id)
            {
                table.active.remove(&identity);
            }
        }
    }

    async fn pump(self, id: String, mut updates: mpsc::Receiver<AuthFlowUpdate>) {
        while let Some(update) = updates.recv().await {
            match update {
                AuthFlowUpdate::Step {
                    state,
                    step,
                    expires_in_secs,
                } => self.advance(&id, state, step, expires_in_secs),
                AuthFlowUpdate::Completed => return self.complete(&id).await,
                AuthFlowUpdate::Failed {
                    code,
                    message,
                    retryable,
                } => {
                    let step = AuthStep::Failed {
                        code,
                        message: sanitize_output(&message),
                        retryable,
                    };
                    return self.finish(&id, AuthSessionState::Failed, step);
                }
                AuthFlowUpdate::Cancelled { message } => {
                    let step = AuthStep::Cancelled {
                        message: message.as_deref().map(sanitize_output),
                    };
                    return self.finish(&id, AuthSessionState::Cancelled, step);
                }
            }
        }
        let started = self
            .table()
            .sessions
            .get(&id)
            .is_some_and(|entry| entry.started);
        if started {
            let step = AuthStep::Failed {
                code: AuthErrorCode::InternalError,
                message: "Login flow ended without a result".into(),
                retryable: true,
            };
            self.finish(&id, AuthSessionState::Failed, step);
        }
    }

    fn advance(
        &self,
        id: &str,
        state: AuthSessionState,
        step: AuthStep,
        expires_in_secs: Option<u64>,
    ) {
        if state.is_terminal() {
            tracing::warn!(?state, "Ignoring a terminal login step from a client");
            return;
        }
        let mut table = self.table();
        let Some(entry) = table
            .sessions
            .get_mut(id)
            .filter(|entry| !entry.snapshot.state.is_terminal())
        else {
            return;
        };
        if let Some(seconds) = expires_in_secs {
            let seconds = seconds.clamp(MIN_PROVIDER_TTL_SECS, MAX_PROVIDER_TTL_SECS);
            let lifetime = Duration::from_secs(seconds).min(self.inner.limits.active_ttl);
            let deadline = Instant::now() + lifetime;
            if deadline < entry.deadline {
                entry.deadline = deadline;
                entry.snapshot.expires_at = rfc3339(pontia_core::time::utc_now() + lifetime);
            }
        }
        entry.update(state, step);
    }

    fn finish(&self, id: &str, state: AuthSessionState, step: AuthStep) {
        let retention = self.inner.limits.terminal_retention;
        // The flow has ended on its own, so its cancellation is not needed.
        drop(settle(&mut self.table(), id, state, step, retention));
    }

    async fn complete(&self, id: &str) {
        let identity = {
            let mut table = self.table();
            let Some(entry) = table
                .sessions
                .get_mut(id)
                .filter(|entry| !entry.snapshot.state.is_terminal())
            else {
                return;
            };
            entry.update(
                AuthSessionState::Completing,
                AuthStep::Completing {
                    message: Some("Verifying authentication status".into()),
                },
            );
            (
                entry.snapshot.client_type.clone(),
                entry.snapshot.client_key.clone(),
            )
        };
        let (client, key) = &identity;
        self.clear_model_catalog(client, key).await;
        let Some(auth) = self
            .inner
            .clients
            .get(client)
            .and_then(|registration| registration.auth.clone())
        else {
            return;
        };
        let mut status = self.probe(auth.as_ref(), key).await;
        if status.state == AuthState::Unknown {
            tokio::time::sleep(STATUS_RETRY_DELAY).await;
            status = self.probe(auth.as_ref(), key).await;
        }
        let succeeded = !matches!(
            status.state,
            AuthState::Unauthenticated | AuthState::Expired
        );
        let (state, step) = if succeeded {
            (
                AuthSessionState::Succeeded,
                AuthStep::Succeeded {
                    status: Box::new(status),
                },
            )
        } else {
            (
                AuthSessionState::Failed,
                AuthStep::Failed {
                    code: AuthErrorCode::ProviderRejected,
                    message: "Login exited without creating a valid credential".into(),
                    retryable: true,
                },
            )
        };
        let settled = settle(
            &mut self.table(),
            id,
            state,
            step,
            self.inner.limits.terminal_retention,
        );
        if settled.is_some() && succeeded {
            self.warm_model_catalog(identity);
        }
    }

    async fn clear_model_catalog(&self, client: &str, key: &str) {
        if let Err(error) = SqliteAgentClientRepository::new(self.inner.db.clone())
            .clear_model_catalog(client, key)
            .await
        {
            tracing::warn!(%error, %client, %key, "Cannot clear client model catalog");
        }
    }

    /// New Sessions see the new account's models without waiting for a refresh.
    fn warm_model_catalog(&self, (client, key): ClientId) {
        let supports_model = self
            .inner
            .clients
            .get(&client)
            .and_then(|registration| registration.configuration.as_ref())
            .is_some_and(|configuration| configuration.supports_startup_model());
        if !supports_model {
            return;
        }
        let db = self.inner.db.clone();
        let service = AgentClientService::new(db.clone(), self.inner.clients.clone());
        let home = self.inner.pontia_home.clone();
        tokio::spawn(async move {
            let enabled = SqliteAgentClientRepository::new(db)
                .get(&client, &key)
                .await
                .ok()
                .flatten()
                .is_some_and(|row| row.enabled);
            if enabled && let Err(error) = service.model_catalog(&client, &key, &home, false).await
            {
                tracing::warn!(%error, %client, %key, "Cannot refresh client models after login");
            }
        });
    }

    pub fn snapshot(&self, client: &str, key: &str, id: &str) -> Result<AuthSessionSnapshot> {
        self.table()
            .sessions
            .get(id)
            .filter(|entry| entry.owned_by(client, key))
            .map(|entry| entry.snapshot.clone())
            .ok_or_else(not_found)
    }

    /// The current snapshot and every later update, without a gap between them.
    pub fn subscribe(
        &self,
        client: &str,
        key: &str,
        id: &str,
    ) -> Result<(
        AuthSessionSnapshot,
        broadcast::Receiver<AuthSessionSnapshot>,
    )> {
        self.table()
            .sessions
            .get(id)
            .filter(|entry| entry.owned_by(client, key))
            .map(|entry| (entry.snapshot.clone(), entry.events.subscribe()))
            .ok_or_else(not_found)
    }

    pub async fn submit_input(
        &self,
        client: &str,
        key: &str,
        id: &str,
        input: AuthInput,
    ) -> Result<AuthSessionSnapshot> {
        let sender = {
            let table = self.table();
            let entry = table
                .sessions
                .get(id)
                .filter(|entry| entry.owned_by(client, key))
                .ok_or_else(not_found)?;
            if entry.snapshot.state.is_terminal() {
                return Err(conflict(
                    "auth_session_already_settled",
                    "Login session has already ended",
                ));
            }
            entry
                .input
                .clone()
                .filter(|_| entry.snapshot.state == AuthSessionState::AwaitingUser)
                .ok_or_else(|| {
                    conflict(
                        "auth_input_not_expected",
                        "The current login step does not accept input",
                    )
                })?
        };
        tokio::time::timeout(INPUT_TIMEOUT, sender.send(input))
            .await
            .ok()
            .and_then(|sent| sent.ok())
            .ok_or_else(|| {
                conflict(
                    "auth_input_not_expected",
                    "Login session no longer accepts input",
                )
            })?;
        self.snapshot(client, key, id)
    }

    pub async fn cancel(&self, client: &str, key: &str, id: &str) -> Result<AuthSessionSnapshot> {
        let (snapshot, cancel) = {
            let mut table = self.table();
            let entry = table
                .sessions
                .get(id)
                .filter(|entry| entry.owned_by(client, key))
                .ok_or_else(not_found)?;
            match entry.snapshot.state {
                AuthSessionState::Cancelled => return Ok(entry.snapshot.clone()),
                state if state.is_terminal() => {
                    return Err(conflict(
                        "auth_session_already_settled",
                        "Login session has already ended",
                    ));
                }
                _ => {}
            }
            settle(
                &mut table,
                id,
                AuthSessionState::Cancelled,
                AuthStep::Cancelled {
                    message: Some("Login cancelled".into()),
                },
                self.inner.limits.terminal_retention,
            )
            .ok_or_else(not_found)?
        };
        if let Some(cancel) = cancel {
            run_cancel(cancel, CANCEL_TIMEOUT).await;
        }
        Ok(snapshot)
    }

    pub async fn logout(
        &self,
        client: &str,
        key: &str,
        scope: Option<&str>,
    ) -> Result<ClientAuthStatus> {
        let auth = self.managed_auth(client, key).await?;
        let identity = (client.to_owned(), key.to_owned());
        {
            let mut table = self.table();
            if table.active.contains_key(&identity) {
                return Err(conflict(
                    "auth_session_in_progress",
                    "Cancel the login session in progress before logging out",
                ));
            }
            if !table.logouts.insert(identity.clone()) {
                return Err(conflict(
                    "auth_operation_in_progress",
                    "Logout is already in progress for this client",
                ));
            }
        }
        let context = self.context();
        let result = tokio::time::timeout(LOGOUT_TIMEOUT, auth.logout(key, scope, &context))
            .await
            .unwrap_or_else(|_| Err(Error::CapabilityUnavailable("Logout timed out".into())));
        self.table().logouts.remove(&identity);
        result?;
        self.clear_model_catalog(client, key).await;
        let mut status = self.probe(auth.as_ref(), key).await;
        status.active_session_id = self.active_session(client, key);
        Ok(status)
    }

    fn ensure_reaper(&self) {
        self.inner.reaper.call_once(|| {
            tokio::spawn(reap(
                Arc::downgrade(&self.inner),
                self.inner.shutdown.subscribe(),
                self.inner.limits.reaper_interval,
            ));
        });
    }

    fn sweep(&self) -> Vec<AuthCancel> {
        let now = Instant::now();
        let mut table = self.table();
        let expired: Vec<String> = table
            .sessions
            .iter()
            .filter(|(_, entry)| !entry.snapshot.state.is_terminal() && now >= entry.deadline)
            .map(|(id, _)| id.clone())
            .collect();
        let mut cancels = Vec::new();
        for id in expired {
            let step = AuthStep::Failed {
                code: AuthErrorCode::Timeout,
                message: "Authentication session timed out".into(),
                retryable: true,
            };
            if let Some((_, Some(cancel))) = settle(
                &mut table,
                &id,
                AuthSessionState::TimedOut,
                step,
                self.inner.limits.terminal_retention,
            ) {
                cancels.push(cancel);
            }
        }
        table
            .sessions
            .retain(|_, entry| entry.evict_at.is_none_or(|evict_at| now < evict_at));
        cancels
    }

    fn cancel_everything(&self) -> Vec<AuthCancel> {
        let mut table = self.table();
        let active: Vec<String> = table.active.values().cloned().collect();
        active
            .into_iter()
            .filter_map(|id| {
                settle(
                    &mut table,
                    &id,
                    AuthSessionState::Cancelled,
                    AuthStep::Cancelled {
                        message: Some("Pontia is shutting down".into()),
                    },
                    self.inner.limits.terminal_retention,
                )
            })
            .filter_map(|(_, cancel)| cancel)
            .collect()
    }
}

async fn cancel_all(cancels: Vec<AuthCancel>, limit: Duration) {
    let mut tasks = JoinSet::new();
    for cancel in cancels {
        tasks.spawn(run_cancel(cancel, limit));
    }
    if tokio::time::timeout(limit, tasks.join_all()).await.is_err() {
        tracing::warn!("Login flow cancellation did not finish before shutdown");
    }
}

async fn reap(inner: Weak<Inner>, mut shutdown: watch::Receiver<bool>, interval: Duration) {
    let mut ticker = tokio::time::interval(interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        let stopping = *shutdown.borrow_and_update();
        if !stopping {
            tokio::select! {
                _ = ticker.tick() => {}
                changed = shutdown.changed() => if changed.is_err() { return },
            }
        }
        let Some(inner) = inner.upgrade() else {
            return;
        };
        let service = ClientAuthService { inner };
        if *shutdown.borrow() {
            cancel_all(service.cancel_everything(), SHUTDOWN_CANCEL_TIMEOUT).await;
            return;
        }
        cancel_all(service.sweep(), CANCEL_TIMEOUT).await;
    }
}
