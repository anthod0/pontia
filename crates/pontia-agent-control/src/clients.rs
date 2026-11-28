mod input;
pub use crate::client_contract::{
    BranchTargetRequest, ClientData, ClientExitMode, ClientExitOutcome, ClientLaunchRequest,
    ClientOperation, ClientSession, ClientSessionDetails, NativeEventEvidence,
};
mod compaction;
mod lifecycle;
mod reasoning_effort;
pub(crate) use lifecycle::discard_unbound_runtime;
mod registry;
pub use registry::{ClientRegistration, ClientRegistry, ClientServiceTasks};

use crate::EventIngestService;
use crate::client_contract::AgentClientSpec;
use pontia_core::{Error, Result};

/// Selects a registered client and executes its application-owned control operations.
#[derive(Clone)]
pub(crate) struct ClientExecutionService {
    pool: sqlx::SqlitePool,
    registry: ClientRegistry,
    events: EventIngestService,
    pontia_home: std::path::PathBuf,
}

impl ClientExecutionService {
    pub(crate) fn registry(&self) -> &ClientRegistry {
        &self.registry
    }
    pub(crate) fn new(
        pool: sqlx::SqlitePool,
        registry: ClientRegistry,
        events: EventIngestService,
        pontia_home: std::path::PathBuf,
    ) -> Self {
        Self {
            pool,
            registry,
            events,
            pontia_home,
        }
    }

    pub(crate) fn for_client(&self, client: &str) -> Result<ClientAdapter> {
        Ok(ClientAdapter {
            spec: self.registry.spec(client).ok_or_else(|| {
                Error::Domain(format!(
                    "client {client} is currently unavailable (not registered)"
                ))
            })?,
            pool: self.pool.clone(),
            registry: self.registry.clone(),
            events: self.events.clone(),
            pontia_home: self.pontia_home.clone(),
        })
    }
}

pub(crate) struct ClientAdapter {
    pub spec: &'static AgentClientSpec,
    pool: sqlx::SqlitePool,
    registry: ClientRegistry,
    events: EventIngestService,
    pontia_home: std::path::PathBuf,
}

impl ClientAdapter {
    pub(crate) fn session_client(&self) -> Option<std::sync::Arc<dyn ClientSession>> {
        self.registry
            .get(self.spec.client_type)
            .and_then(|entry| entry.session.clone())
    }

    pub fn prepares_on_input(&self) -> bool {
        self.registry
            .get(self.spec.client_type)
            .is_some_and(|entry| entry.prepare_on_input)
    }
    pub fn client_owns_turn(&self) -> bool {
        self.prepares_on_input() || self.spec.owns_interactive_turn()
    }
    pub fn supports_restart(&self) -> bool {
        self.spec.adapter.runtime != crate::client_contract::RuntimeBehavior::External
    }

    pub async fn input_available(&self, session: &str) -> Result<bool> {
        self.session_client()
            .ok_or_else(|| {
                Error::CapabilityUnavailable(format!(
                    "Client {} has no Session control implementation",
                    self.spec.client_type
                ))
            })?
            .available(self.pool.clone(), session)
            .await
    }
}

#[cfg(test)]
pub(crate) mod testing;
