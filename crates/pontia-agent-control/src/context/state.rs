use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use sqlx::SqlitePool;

use super::{AgentControlBuilder, ShutdownSignal};
use crate::{AgentEventBroker, LiveOutputService, live_output::LiveOutputStore};

#[derive(Clone)]
pub struct AgentControl {
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
    client_control: crate::ClientControlService,
}

struct CommandServices {
    sessions: crate::SessionCommandService,
    turns: crate::TurnCommandService,
    inbox: Arc<crate::InboxCommandService>,
}

impl AgentControl {
    pub fn builder(db: SqlitePool, pontia_home: PathBuf) -> AgentControlBuilder {
        AgentControlBuilder::new(db, pontia_home)
    }

    pub(super) fn from_builder(mut builder: AgentControlBuilder) -> Self {
        builder.client_control = builder.client_control.with_clients(builder.clients.clone());
        let scheduler = crate::inbox::InboxScheduler::default();
        let ingest = crate::EventIngestService::new(
            builder.db.clone(),
            builder.clients.clone(),
            crate::ingestion::PostCommitEffects::new(
                builder.client_control.clone(),
                builder.agent_events.clone(),
                crate::LiveOutputService::new(builder.db.clone(), builder.live_output.clone())
                    .with_clients(builder.clients.clone()),
                scheduler.clone(),
            ),
        );
        let queries = crate::ControlQueryService::new(builder.db.clone())
            .with_clients(builder.clients.clone())
            .with_control(builder.client_control.clone());
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
            .with_clients(builder.clients.clone())
            .with_history_control(builder.client_control.clone());
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
        let mut sessions = crate::SessionCommandService::new(
            builder.db.clone(),
            ingest.clone(),
            queries,
            execution,
            turns.clone(),
            inbox.clone(),
            builder.pontia_home.clone(),
        );
        if let Some(environment) = builder.environment {
            sessions = sessions.with_environment(environment);
        }
        Self {
            inner: Arc::new(AgentControlInner {
                commands: CommandServices {
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
                    ingest,
                    agent_events: builder.agent_events,
                    interactions: crate::interactions::InteractionService::new(builder.db.clone()),
                    live_output: builder.live_output,
                },
                lifecycle: LifecycleState {
                    shutdown: builder.shutdown,
                },
                integrations: IntegrationState {
                    clients: builder.clients,
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

    pub fn client_control(&self) -> crate::ClientControlService {
        self.inner.integrations.client_control.clone()
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
    pub fn queries(&self) -> crate::ControlQueryService {
        crate::ControlQueryService::new(self.db())
            .with_clients(self.clients())
            .with_control(self.client_control())
    }
    pub fn runtime_observer(&self) -> crate::RuntimeObservationService {
        crate::RuntimeObservationService::new(
            self.db(),
            self.clients(),
            self.event_ingest_service(),
            self.client_control(),
        )
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
