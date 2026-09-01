use std::path::PathBuf;

use pontia_config::{FilePickerConfig, WorkspaceBrowserConfig};
use sqlx::SqlitePool;

use super::{AppState, ShutdownSignal, VolatileEventBroker};
use crate::{AgentEventBroker, GitRefreshCoordinator, IdempotencyCoordinator};

pub struct AppStateBuilder {
    pub(super) db: SqlitePool,
    pub(super) pontia_home: PathBuf,
    pub(super) external_api_token: Option<String>,
    pub(super) workspace_browser: WorkspaceBrowserConfig,
    pub(super) file_picker: FilePickerConfig,
    pub(super) shutdown: ShutdownSignal,
    pub(super) agent_events: AgentEventBroker,
    pub(super) volatile_events: VolatileEventBroker,
    pub(super) git_refresh: GitRefreshCoordinator,
    pub(super) idempotency: IdempotencyCoordinator,
}

impl AppStateBuilder {
    pub(super) fn new(db: SqlitePool, pontia_home: PathBuf) -> Self {
        Self {
            db,
            pontia_home,
            external_api_token: None,
            workspace_browser: WorkspaceBrowserConfig::default(),
            file_picker: FilePickerConfig::default(),
            shutdown: ShutdownSignal::default(),
            agent_events: AgentEventBroker::default(),
            volatile_events: VolatileEventBroker::default(),
            git_refresh: GitRefreshCoordinator::default(),
            idempotency: IdempotencyCoordinator::default(),
        }
    }

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

    pub fn agent_events(mut self, agent_events: AgentEventBroker) -> Self {
        self.agent_events = agent_events;
        self
    }

    pub fn git_refresh(mut self, git_refresh: GitRefreshCoordinator) -> Self {
        self.git_refresh = git_refresh;
        self
    }

    pub fn idempotency(mut self, idempotency: IdempotencyCoordinator) -> Self {
        self.idempotency = idempotency;
        self
    }

    pub fn build(self) -> AppState {
        AppState::from_builder(self)
    }
}
