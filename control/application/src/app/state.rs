use std::sync::Arc;

use pontia_config::{FilePickerConfig, WorkspaceBrowserConfig};
use sqlx::SqlitePool;

use super::{AppStateBuilder, ShutdownSignal, VolatileEventBroker};
use crate::{AgentEventBroker, ApprovalCoordinator, GitRefreshCoordinator, IdempotencyCoordinator};

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
    external_api_token: Option<String>,
    workspace_browser: WorkspaceBrowserConfig,
    file_picker: FilePickerConfig,
}

struct EventState {
    agent_events: AgentEventBroker,
    volatile_events: VolatileEventBroker,
}

struct LifecycleState {
    shutdown: ShutdownSignal,
}

struct IntegrationState {
    approvals: ApprovalCoordinator,
    git_refresh: GitRefreshCoordinator,
    idempotency: IdempotencyCoordinator,
}

impl AppState {
    pub fn builder(db: SqlitePool) -> AppStateBuilder {
        AppStateBuilder::new(db)
    }

    pub(super) fn from_builder(builder: AppStateBuilder) -> Self {
        Self {
            inner: Arc::new(AppStateInner {
                persistence: PersistenceState { db: builder.db },
                config: AppRuntimeState {
                    external_api_token: builder.external_api_token,
                    workspace_browser: builder.workspace_browser,
                    file_picker: builder.file_picker,
                },
                events: EventState {
                    agent_events: builder.agent_events,
                    volatile_events: builder.volatile_events,
                },
                lifecycle: LifecycleState {
                    shutdown: builder.shutdown,
                },
                integrations: IntegrationState {
                    approvals: builder.approvals,
                    git_refresh: builder.git_refresh,
                    idempotency: builder.idempotency,
                },
            }),
        }
    }

    pub fn db(&self) -> SqlitePool {
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

    pub fn agent_events(&self) -> AgentEventBroker {
        self.inner.events.agent_events.clone()
    }

    pub fn git_refresh(&self) -> GitRefreshCoordinator {
        self.inner.integrations.git_refresh.clone()
    }

    pub fn idempotency(&self) -> IdempotencyCoordinator {
        self.inner.integrations.idempotency.clone()
    }

    pub fn approvals(&self) -> ApprovalCoordinator {
        self.inner.integrations.approvals.clone()
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
            .agent_events(self.agent_events())
            .volatile_events(self.volatile_events())
            .git_refresh(self.git_refresh())
            .idempotency(self.idempotency())
            .approvals(self.approvals())
    }
}
