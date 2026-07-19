use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use pontia_config::{AppConfig, FilePickerConfig};
use pontia_core::{domain::DomainEvent, error::Result};
use pontia_runtime::{set_runtime_bind_addr, set_runtime_config};
use pontia_storage_sqlite::{connect_sqlite, run_migrations};

use super::set_default_client_type;
use crate::{GitRefreshCoordinator, WorkspaceBrowserConfig};

const SESSION_MESSAGE_UPDATED_DEBOUNCE_MS: u64 = 100;

#[derive(Clone)]
pub struct VolatileEventBroker {
    sender: tokio::sync::broadcast::Sender<DomainEvent>,
    debounced_session_message_updates: Arc<Mutex<HashMap<String, DebouncedVolatileEvent>>>,
    next_debounce_generation: Arc<AtomicU64>,
}

struct DebouncedVolatileEvent {
    generation: u64,
    task: tokio::task::JoinHandle<()>,
}

impl VolatileEventBroker {
    pub fn publish(&self, event: DomainEvent) {
        let _ = self.sender.send(event);
    }

    pub fn publish_debounced_session_message_updated(&self, event: DomainEvent) {
        let session_id = event.session_id.clone();
        let generation = self
            .next_debounce_generation
            .fetch_add(1, Ordering::Relaxed);
        let sender = self.sender.clone();
        let pending_updates = self.debounced_session_message_updates.clone();
        let task_session_id = session_id.clone();
        let task = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(SESSION_MESSAGE_UPDATED_DEBOUNCE_MS)).await;
            let _ = sender.send(event);
            let mut pending = pending_updates
                .lock()
                .expect("volatile event debounce lock poisoned");
            if pending
                .get(&task_session_id)
                .is_some_and(|pending| pending.generation == generation)
            {
                pending.remove(&task_session_id);
            }
        });

        let mut pending = self
            .debounced_session_message_updates
            .lock()
            .expect("volatile event debounce lock poisoned");
        if let Some(previous) =
            pending.insert(session_id, DebouncedVolatileEvent { generation, task })
        {
            previous.task.abort();
        }
    }

    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<DomainEvent> {
        self.sender.subscribe()
    }
}

impl Default for VolatileEventBroker {
    fn default() -> Self {
        let (sender, _) = tokio::sync::broadcast::channel(1024);
        Self {
            sender,
            debounced_session_message_updates: Arc::new(Mutex::new(HashMap::new())),
            next_debounce_generation: Arc::new(AtomicU64::new(1)),
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    inner: Arc<AppStateInner>,
}

struct AppStateInner {
    persistence: PersistenceState,
    config: AppRuntimeState,
    events: EventState,
    lifecycle: LifecycleState,
    integrations: IntegrationState,
}

struct PersistenceState {
    db: sqlx::SqlitePool,
}

struct AppRuntimeState {
    external_api_token: Option<String>,
    workspace_browser: WorkspaceBrowserConfig,
    file_picker: FilePickerConfig,
}

struct EventState {
    volatile_events: VolatileEventBroker,
}

struct LifecycleState {
    shutdown: ShutdownSignal,
}

struct IntegrationState {
    git_refresh: GitRefreshCoordinator,
}

pub struct AppStateBuilder {
    db: sqlx::SqlitePool,
    external_api_token: Option<String>,
    workspace_browser: WorkspaceBrowserConfig,
    file_picker: FilePickerConfig,
    shutdown: ShutdownSignal,
    volatile_events: VolatileEventBroker,
    git_refresh: GitRefreshCoordinator,
}

impl AppState {
    pub fn builder(db: sqlx::SqlitePool) -> AppStateBuilder {
        AppStateBuilder {
            db,
            external_api_token: None,
            workspace_browser: WorkspaceBrowserConfig::default(),
            file_picker: FilePickerConfig::default(),
            shutdown: ShutdownSignal::default(),
            volatile_events: VolatileEventBroker::default(),
            git_refresh: GitRefreshCoordinator::default(),
        }
    }

    pub fn db(&self) -> sqlx::SqlitePool {
        self.inner.persistence.db.clone()
    }

    pub fn external_api_token(&self) -> Option<&str> {
        self.inner.config.external_api_token.as_deref()
    }

    pub fn workspace_browser(&self) -> WorkspaceBrowserConfig {
        self.inner.config.workspace_browser.clone()
    }

    pub fn file_picker(&self) -> FilePickerConfig {
        self.inner.config.file_picker.clone()
    }

    pub fn shutdown(&self) -> ShutdownSignal {
        self.inner.lifecycle.shutdown.clone()
    }

    pub fn volatile_events(&self) -> VolatileEventBroker {
        self.inner.events.volatile_events.clone()
    }

    pub fn git_refresh(&self) -> GitRefreshCoordinator {
        self.inner.integrations.git_refresh.clone()
    }

    pub fn with_external_api_token(&self, external_api_token: Option<String>) -> Self {
        self.rebuild()
            .external_api_token(external_api_token)
            .build()
    }

    fn rebuild(&self) -> AppStateBuilder {
        AppState::builder(self.db())
            .external_api_token(self.inner.config.external_api_token.clone())
            .workspace_browser(self.workspace_browser())
            .file_picker(self.file_picker())
            .shutdown(self.shutdown())
            .volatile_events(self.volatile_events())
            .git_refresh(self.git_refresh())
    }
}

impl AppStateBuilder {
    pub fn external_api_token(mut self, external_api_token: Option<String>) -> Self {
        self.external_api_token = external_api_token;
        self
    }

    pub fn workspace_browser(mut self, workspace_browser: WorkspaceBrowserConfig) -> Self {
        self.workspace_browser = workspace_browser;
        self
    }

    pub fn file_picker(mut self, file_picker: FilePickerConfig) -> Self {
        self.file_picker = file_picker;
        self
    }

    pub fn shutdown(mut self, shutdown: ShutdownSignal) -> Self {
        self.shutdown = shutdown;
        self
    }

    pub fn volatile_events(mut self, volatile_events: VolatileEventBroker) -> Self {
        self.volatile_events = volatile_events;
        self
    }

    pub fn git_refresh(mut self, git_refresh: GitRefreshCoordinator) -> Self {
        self.git_refresh = git_refresh;
        self
    }

    pub fn build(self) -> AppState {
        AppState {
            inner: Arc::new(AppStateInner {
                persistence: PersistenceState { db: self.db },
                config: AppRuntimeState {
                    external_api_token: self.external_api_token,
                    workspace_browser: self.workspace_browser,
                    file_picker: self.file_picker,
                },
                events: EventState {
                    volatile_events: self.volatile_events,
                },
                lifecycle: LifecycleState {
                    shutdown: self.shutdown,
                },
                integrations: IntegrationState {
                    git_refresh: self.git_refresh,
                },
            }),
        }
    }
}

#[derive(Clone)]
pub struct ShutdownSignal {
    sender: tokio::sync::watch::Sender<bool>,
}

impl ShutdownSignal {
    pub fn notify(&self) {
        let _ = self.sender.send(true);
    }

    pub fn subscribe(&self) -> tokio::sync::watch::Receiver<bool> {
        self.sender.subscribe()
    }
}

impl Default for ShutdownSignal {
    fn default() -> Self {
        let (sender, _) = tokio::sync::watch::channel(false);
        Self { sender }
    }
}

pub async fn initialize(config: &AppConfig) -> Result<AppState> {
    let db = connect_sqlite(&config.database_url).await?;

    if config.run_migrations {
        run_migrations(&db).await?;
    }

    set_default_client_type(config.default_client_type.clone());
    set_runtime_config(config.runtime.clone());
    set_runtime_bind_addr(config.bind_addr);
    Ok(AppState::builder(db)
        .external_api_token(config.external_api_token.clone())
        .workspace_browser(config.workspace_browser.clone())
        .file_picker(config.file_picker.clone())
        .build())
}
