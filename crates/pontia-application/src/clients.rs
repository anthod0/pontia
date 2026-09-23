mod input;
pub use crate::client_contract::{
    BranchTargetRequest, ClientData, ClientLaunchRequest, ClientLauncher, ClientOperation,
    ClientSession, ClientSessionDetails, InProcessClient, NativeEventEvidence,
};
mod connections;
pub use connections::ClientControlService;
mod lifecycle;
mod models;
pub(crate) use lifecycle::discard_unbound_runtime;
mod channel;
mod registry;
pub use registry::{ClientRegistration, ClientRegistry};

use crate::EventIngestService;
use crate::client_contract::{AgentClientSpec, DispatchMode};
use pontia_core::{Error, Result};

/// Selects a registered client and executes its application-owned control operations.
#[derive(Clone)]
pub(crate) struct ClientExecutionService {
    pool: sqlx::SqlitePool,
    registry: ClientRegistry,
    events: EventIngestService,
    control: ClientControlService,
}

impl ClientExecutionService {
    pub(crate) fn new(
        pool: sqlx::SqlitePool,
        registry: ClientRegistry,
        events: EventIngestService,
        control: ClientControlService,
    ) -> Self {
        Self {
            pool,
            registry,
            events,
            control,
        }
    }

    pub(crate) fn for_client(&self, client: &str) -> Result<ClientAdapter> {
        Ok(ClientAdapter {
            spec: self
                .registry
                .spec(client)
                .ok_or_else(|| Error::Domain(format!("unsupported client_type: {client}")))?,
            pool: self.pool.clone(),
            registry: self.registry.clone(),
            events: self.events.clone(),
            control: self.control.clone(),
        })
    }
}

pub(crate) struct ClientAdapter {
    pub spec: &'static AgentClientSpec,
    pool: sqlx::SqlitePool,
    registry: ClientRegistry,
    events: EventIngestService,
    control: ClientControlService,
}

impl ClientAdapter {
    fn session_client(&self) -> Option<std::sync::Arc<dyn ClientSession>> {
        self.registry
            .get(self.spec.client_type)
            .and_then(|entry| entry.session.clone())
    }

    pub fn supports_steer(&self) -> bool {
        self.registry
            .get(self.spec.client_type)
            .is_some_and(|entry| entry.steer)
    }
    pub fn prepares_on_input(&self) -> bool {
        self.registry
            .get(self.spec.client_type)
            .is_some_and(|entry| entry.prepare_on_input)
    }
    pub fn client_owns_turn(&self) -> bool {
        self.prepares_on_input() || self.spec.owns_interactive_tmux_turn()
    }
    pub fn supports_restart(&self) -> bool {
        self.spec.adapter.runtime != crate::client_contract::RuntimeBehavior::External
    }

    pub async fn input_available(&self, session: &str) -> Result<bool> {
        if let Some(client) = self.session_client() {
            return client.available(self.pool.clone(), session).await;
        }
        if self.spec.adapter.dispatch == DispatchMode::Connected {
            return self.control.available(session).await;
        }
        Ok(true)
    }
}

#[cfg(test)]
pub(crate) mod testing;
