use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use pontia_config::{FilePickerConfig, WorkspaceBrowserConfig};
use sqlx::SqlitePool;

use super::{AppStateBuilder, ShutdownSignal, VolatileEventBroker};
use crate::{
    AgentEventBroker, GitRefreshCoordinator, IdempotencyCoordinator, LiveOutputService,
    live_output::LiveOutputStore,
};

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
    db: SqlitePool,
}

struct AppRuntimeState {
    pontia_home: PathBuf,
    external_api_token: Option<String>,
    workspace_browser: WorkspaceBrowserConfig,
    file_picker: FilePickerConfig,
}

struct EventState {
    ingest: crate::EventIngestService,
    agent_events: AgentEventBroker,
    volatile_events: VolatileEventBroker,
    live_output: LiveOutputStore,
}

struct LifecycleState {
    shutdown: ShutdownSignal,
}

struct IntegrationState {
    clients: crate::clients::ClientRegistry,
    git_refresh: GitRefreshCoordinator,
    idempotency: IdempotencyCoordinator,
    client_control: crate::ClientControlService,
}

impl AppState {
    pub fn builder(db: SqlitePool, pontia_home: PathBuf) -> AppStateBuilder {
        AppStateBuilder::new(db, pontia_home)
    }

    pub(super) fn from_builder(builder: AppStateBuilder) -> Self {
        let ingest = crate::EventIngestService::new(builder.db.clone())
            .with_clients(builder.clients.clone())
            .with_reporting_dependencies(
                builder.client_control.clone(),
                builder.agent_events.clone(),
                crate::LiveOutputService::new(builder.db.clone(), builder.live_output.clone())
                    .with_clients(builder.clients.clone()),
                builder.volatile_events.clone(),
            );
        Self {
            inner: Arc::new(AppStateInner {
                persistence: PersistenceState { db: builder.db },
                config: AppRuntimeState {
                    pontia_home: builder.pontia_home,
                    external_api_token: builder.external_api_token,
                    workspace_browser: builder.workspace_browser,
                    file_picker: builder.file_picker,
                },
                events: EventState {
                    ingest,
                    agent_events: builder.agent_events,
                    volatile_events: builder.volatile_events,
                    live_output: builder.live_output,
                },
                lifecycle: LifecycleState {
                    shutdown: builder.shutdown,
                },
                integrations: IntegrationState {
                    clients: builder.clients,
                    git_refresh: builder.git_refresh,
                    idempotency: builder.idempotency,
                    client_control: builder.client_control,
                },
            }),
        }
    }

    pub fn clients(&self) -> crate::clients::ClientRegistry {
        self.inner.integrations.clients.clone()
    }

    pub fn db(&self) -> SqlitePool {
        self.inner.persistence.db.clone()
    }

    pub fn pontia_home(&self) -> &Path {
        &self.inner.config.pontia_home
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

    pub fn agent_events(&self) -> AgentEventBroker {
        self.inner.events.agent_events.clone()
    }

    pub fn live_output(&self) -> LiveOutputService {
        LiveOutputService::new(self.db(), self.inner.events.live_output.clone())
            .with_clients(self.clients())
    }

    pub fn event_ingest_service(&self) -> crate::EventIngestService {
        self.inner.events.ingest.clone()
    }

    pub fn git_refresh(&self) -> GitRefreshCoordinator {
        self.inner.integrations.git_refresh.clone()
    }

    pub fn client_control(&self) -> crate::ClientControlService {
        self.inner.integrations.client_control.clone()
    }

    pub fn idempotency(&self) -> IdempotencyCoordinator {
        self.inner.integrations.idempotency.clone()
    }

    pub fn with_external_api_token(&self, external_api_token: Option<String>) -> Self {
        self.rebuild()
            .external_api_token(external_api_token)
            .build()
    }

    fn rebuild(&self) -> AppStateBuilder {
        AppState::builder(self.db(), self.inner.config.pontia_home.clone())
            .external_api_token(self.inner.config.external_api_token.clone())
            .workspace_browser(self.workspace_browser())
            .file_picker(self.file_picker())
            .shutdown(self.shutdown())
            .agent_events(self.agent_events())
            .volatile_events(self.volatile_events())
            .live_output(self.inner.events.live_output.clone())
            .git_refresh(self.git_refresh())
            .idempotency(self.idempotency())
            .client_control(self.client_control())
            .clients(self.clients())
    }
}
