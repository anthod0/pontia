use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use sqlx::SqlitePool;

use super::{AgentControlBuilder, ShutdownSignal};
use crate::{AgentEventBroker, LiveOutputService, live_output::LiveOutputStore};

#[derive(Clone)]
pub struct AgentControl(ClientServiceContext);

impl std::ops::Deref for AgentControl {
    type Target = ClientServiceContext;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Clone)]
pub struct ClientServiceContext {
    inner: Arc<AgentControlInner>,
}

struct AgentControlInner {
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
}

struct EventState {
    changes: crate::ControlChanges,
    ingest: crate::EventIngestService,
    agent_events: AgentEventBroker,
    live_output: LiveOutputStore,
    interactions: crate::interactions::InteractionService,
}

struct LifecycleState {
    shutdown: ShutdownSignal,
}

struct IntegrationState {
    clients: crate::clients::ClientRegistry,
    client_auth: crate::client_auth::ClientAuthService,
}

struct CommandServices {
    branches: crate::BranchReplayService,
    sessions: Arc<crate::SessionCommandService>,
    turns: crate::TurnCommandService,
    inbox: Arc<crate::InboxCommandService>,
}

impl AgentControl {
    pub fn builder(db: SqlitePool, pontia_home: PathBuf) -> AgentControlBuilder {
        AgentControlBuilder::new(db, pontia_home)
    }

    pub(super) fn from_builder(builder: AgentControlBuilder) -> Self {
        let scheduler = crate::inbox::InboxScheduler::default();
        let mut ingest = crate::EventIngestService::new(
            builder.db.clone(),
            builder.clients.clone(),
            crate::ingestion::PostCommitEffects::new(
                builder.agent_events.clone(),
                crate::LiveOutputService::new(builder.db.clone(), builder.live_output.clone())
                    .with_clients(builder.clients.clone()),
                scheduler.clone(),
            ),
        );
        ingest.cache = Some(crate::timeline_cache::TimelineCache::new(
            builder.db.clone(),
            &builder.pontia_home,
        ));
        let queries = crate::ControlQueryService::new(builder.db.clone())
            .with_clients(builder.clients.clone());
        let execution = crate::clients::ClientExecutionService::new(
            builder.db.clone(),
            builder.clients.clone(),
            ingest.clone(),
            builder.pontia_home.clone(),
        );
        let turns = crate::TurnCommandService::new(
            builder.db.clone(),
            ingest.clone(),
            queries.clone(),
            execution.clone(),
            scheduler.clone(),
        );
        let branches = crate::BranchReplayService::new(
            builder.db.clone(),
            builder.clients.clone(),
            execution.clone(),
            scheduler.clone(),
        );
        let inbox = Arc::new(crate::InboxCommandService::new(
            builder.db.clone(),
            queries.clone(),
            execution.clone(),
            turns.clone(),
            scheduler.clone(),
        ));
        let sessions = Arc::new(crate::SessionCommandService::new(
            builder.db.clone(),
            ingest.clone(),
            queries,
            execution,
            turns.clone(),
            inbox.clone(),
            builder.pontia_home.clone(),
        ));
        scheduler.connect(&inbox, &sessions);
        let client_auth = crate::client_auth::ClientAuthService::new(
            builder.db.clone(),
            builder.clients.clone(),
            builder.pontia_home.clone(),
            builder.shutdown.clone(),
        );
        Self(ClientServiceContext {
            inner: Arc::new(AgentControlInner {
                commands: CommandServices {
                    branches,
                    sessions,
                    turns,
                    inbox,
                },
                persistence: PersistenceState {
                    db: builder.db.clone(),
                },
                config: AppRuntimeState {
                    pontia_home: builder.pontia_home,
                },
                events: EventState {
                    changes: Default::default(),
                    ingest: ingest.clone(),
                    agent_events: builder.agent_events,
                    interactions: crate::interactions::InteractionService::new(
                        ingest,
                        builder.shutdown.subscribe(),
                    ),
                    live_output: builder.live_output,
                },
                lifecycle: LifecycleState {
                    shutdown: builder.shutdown,
                },
                integrations: IntegrationState {
                    client_auth,
                    clients: builder.clients,
                },
            }),
        })
    }

    pub fn db(&self) -> SqlitePool {
        self.0.db()
    }
    pub fn client_context(&self) -> ClientServiceContext {
        self.0.clone()
    }
    pub fn client_auth(&self) -> crate::client_auth::ClientAuthService {
        self.inner.integrations.client_auth.clone()
    }

    pub async fn run_timeline_cache_cleanup(&self) {
        if let Some(cache) = self.event_ingest_service().cache {
            cache.run(self.shutdown().subscribe()).await;
        }
    }

    pub fn control_changes(&self) -> crate::ControlChanges {
        self.inner.events.changes.clone()
    }

    pub fn agent_profiles(&self) -> crate::AgentProfileService {
        crate::AgentProfileService::new(
            self.db(),
            self.clients(),
            self.control_changes(),
            self.pontia_home(),
        )
    }

    pub fn session_commands(&self) -> crate::SessionCommandService {
        self.inner.commands.sessions.as_ref().clone()
    }

    pub fn turn_commands(&self) -> crate::TurnCommandService {
        self.inner.commands.turns.clone()
    }

    pub fn runtime_observer(&self) -> crate::RuntimeObservationService {
        crate::RuntimeObservationService::new(
            self.db(),
            self.clients(),
            self.event_ingest_service(),
        )
    }
}

impl ClientServiceContext {
    pub fn store(&self) -> crate::ClientControlStore {
        crate::ClientControlStore::new(self.db())
    }

    pub async fn resolve_target(
        &self,
        session: &str,
        expected: Option<&str>,
    ) -> pontia_core::Result<crate::runtime::ControlTarget> {
        crate::runtime::ControlTarget::resolve(&self.db(), &self.clients(), session, expected).await
    }

    pub fn clients(&self) -> crate::clients::ClientRegistry {
        self.inner.integrations.clients.clone()
    }

    pub(crate) fn db(&self) -> SqlitePool {
        self.inner.persistence.db.clone()
    }

    pub fn pontia_home(&self) -> &Path {
        &self.inner.config.pontia_home
    }

    pub fn shutdown(&self) -> ShutdownSignal {
        self.inner.lifecycle.shutdown.clone()
    }

    pub fn interactions(&self) -> crate::interactions::InteractionService {
        self.inner.events.interactions.clone()
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

    pub fn inbox_commands(&self) -> Arc<crate::InboxCommandService> {
        self.inner.commands.inbox.clone()
    }
    pub fn branch_replays(&self) -> crate::BranchReplayService {
        self.inner.commands.branches.clone()
    }
    pub fn queries(&self) -> crate::ControlQueryService {
        crate::ControlQueryService::new(self.db()).with_clients(self.clients())
    }
    pub fn session_runtimes(&self) -> crate::RuntimeBindingUpsertService {
        crate::RuntimeBindingUpsertService::new(
            self.db(),
            self.pontia_home().into(),
            self.clients(),
            self.event_ingest_service(),
        )
    }
}
