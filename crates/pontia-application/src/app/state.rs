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
    external_api_token: Option<String>,
}

struct AppStateInner {
    persistence: PersistenceState,
    config: AppRuntimeState,
    events: EventState,
    lifecycle: LifecycleState,
    integrations: IntegrationState,
    commands: CommandServices,
}

struct PersistenceState {
    db: SqlitePool,
}

struct AppRuntimeState {
    pontia_home: PathBuf,
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

struct CommandServices {
    sessions: crate::SessionCommandService,
    turns: crate::TurnCommandService,
    inbox: Arc<crate::InboxCommandService>,
    tasks: crate::TaskCommandService,
}

impl AppState {
    pub fn builder(db: SqlitePool, pontia_home: PathBuf) -> AppStateBuilder {
        AppStateBuilder::new(db, pontia_home)
    }

    pub(super) fn from_builder(builder: AppStateBuilder) -> Self {
        let scheduler = crate::inbox::InboxScheduler::default();
        let ingest = crate::EventIngestService::new(
            builder.db.clone(),
            builder.clients.clone(),
            crate::ingestion::PostCommitEffects::new(
                builder.client_control.clone(),
                builder.agent_events.clone(),
                crate::LiveOutputService::new(builder.db.clone(), builder.live_output.clone())
                    .with_clients(builder.clients.clone()),
                builder.volatile_events.clone(),
                scheduler.clone(),
            ),
        );
        let queries = crate::ExternalQueryService::new(builder.db.clone())
            .with_clients(builder.clients.clone());
        let execution = crate::clients::ClientExecutionService::new(
            builder.db.clone(),
            builder.clients.clone(),
            ingest.clone(),
            builder.client_control.clone(),
        );
        let turns = crate::TurnCommandService::new(
            builder.db.clone(),
            ingest.clone(),
            queries.clone(),
            execution.clone(),
            scheduler.clone(),
        );
        let branches = crate::BranchReplayService::new(builder.db.clone())
            .with_clients(builder.clients.clone());
        let inbox = Arc::new(crate::InboxCommandService::new(
            builder.db.clone(),
            ingest.clone(),
            queries.clone(),
            execution.clone(),
            turns.clone(),
            branches,
            scheduler.clone(),
        ));
        scheduler.connect(&inbox);
        let sessions = crate::SessionCommandService::new(
            builder.db.clone(),
            ingest.clone(),
            queries,
            execution,
            turns.clone(),
            inbox.clone(),
            builder.pontia_home.clone(),
        );
        let tasks = crate::TaskCommandService::new(builder.db.clone(), turns.clone());
        Self {
            external_api_token: builder.external_api_token,
            inner: Arc::new(AppStateInner {
                commands: CommandServices {
                    sessions,
                    turns,
                    inbox,
                    tasks,
                },
                persistence: PersistenceState { db: builder.db },
                config: AppRuntimeState {
                    pontia_home: builder.pontia_home,
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
        self.external_api_token.as_deref()
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

    pub fn session_commands(&self) -> crate::SessionCommandService {
        self.inner.commands.sessions.clone()
    }
    pub fn turn_commands(&self) -> crate::TurnCommandService {
        self.inner.commands.turns.clone()
    }
    pub fn inbox_commands(&self) -> Arc<crate::InboxCommandService> {
        self.inner.commands.inbox.clone()
    }
    pub fn task_commands(&self) -> crate::TaskCommandService {
        self.inner.commands.tasks.clone()
    }
    pub fn queries(&self) -> crate::ExternalQueryService {
        crate::ExternalQueryService::new(self.db()).with_clients(self.clients())
    }
    pub fn runtime_observer(&self) -> crate::RuntimeObservationService {
        crate::RuntimeObservationService::new(
            self.db(),
            self.clients(),
            self.event_ingest_service(),
        )
    }
    pub fn runtime_bindings(&self) -> crate::RuntimeBindingUpsertService {
        crate::RuntimeBindingUpsertService::new(
            self.db(),
            self.pontia_home().into(),
            self.clients(),
            self.event_ingest_service(),
        )
    }

    pub fn with_external_api_token(&self, external_api_token: Option<String>) -> Self {
        Self {
            inner: self.inner.clone(),
            external_api_token,
        }
    }
}
