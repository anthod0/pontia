use std::path::PathBuf;

use sqlx::SqlitePool;

use super::{AgentControl, ShutdownSignal};
use crate::{AgentEventBroker, live_output::LiveOutputStore};

pub struct AgentControlBuilder {
    pub(super) clients: crate::clients::ClientRegistry,
    pub(super) db: SqlitePool,
    pub(super) pontia_home: PathBuf,
    pub(super) managed_private_roots: Vec<PathBuf>,
    pub(super) shutdown: ShutdownSignal,
    pub(super) agent_events: AgentEventBroker,
    pub(super) live_output: LiveOutputStore,
}

impl AgentControlBuilder {
    pub(super) fn new(db: SqlitePool, pontia_home: PathBuf) -> Self {
        Self {
            clients: Default::default(),
            db,
            pontia_home,
            managed_private_roots: Vec::new(),
            shutdown: ShutdownSignal::default(),
            agent_events: AgentEventBroker::default(),
            live_output: LiveOutputStore::default(),
        }
    }

    pub fn managed_private_root(mut self, root: PathBuf) -> Self {
        self.managed_private_roots.push(root);
        self
    }

    pub fn shutdown(mut self, shutdown: ShutdownSignal) -> Self {
        self.shutdown = shutdown;
        self
    }

    pub fn agent_events(mut self, agent_events: AgentEventBroker) -> Self {
        self.agent_events = agent_events;
        self
    }

    pub fn clients(mut self, clients: crate::clients::ClientRegistry) -> Self {
        self.clients = clients;
        self
    }

    pub fn build(self) -> AgentControl {
        AgentControl::from_builder(self)
    }
}
