mod input;
mod session;
pub use session::{ClientOperation, ClientSession, ClientSessionDetails, InProcessClient};
mod lifecycle;
mod models;
pub(crate) use lifecycle::discard_unbound_runtime;
mod channel;
mod registry;
pub use registry::{
    BranchTargetRequest, ClientData, ClientLaunchRequest, ClientLauncher, ClientRegistration,
    ClientRegistry, NativeEventEvidence,
};

use crate::client_contract::{AgentClientSpec, DispatchMode};
use crate::{ClientControlService, EventIngestService};
use pontia_core::{Error, Result};

/// Client selection and native execution live here. Business policy stays with
/// the service that owns the operation.
pub(crate) struct ClientAdapter {
    pub spec: &'static AgentClientSpec,
    pub events: EventIngestService,
    pub control: Option<ClientControlService>,
}

impl ClientAdapter {
    pub fn new(
        client: &str,
        events: EventIngestService,
        control: Option<ClientControlService>,
    ) -> Result<Self> {
        Ok(Self {
            spec: events
                .clients()
                .spec(client)
                .ok_or_else(|| Error::Domain(format!("unsupported client_type: {client}")))?,
            events,
            control,
        })
    }

    fn session_client(&self) -> Option<std::sync::Arc<dyn ClientSession>> {
        self.events
            .clients()
            .get(self.spec.client_type)
            .and_then(|entry| entry.session.clone())
    }

    pub fn supports_steer(&self) -> bool {
        self.events
            .clients()
            .get(self.spec.client_type)
            .is_some_and(|entry| entry.steer)
    }
    pub fn prepares_on_input(&self) -> bool {
        self.events
            .clients()
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
            return client.available(self.events.db(), session).await;
        }
        if self.spec.adapter.dispatch == DispatchMode::Connected {
            return match &self.control {
                Some(pi) => pi.available(session).await,
                None => Ok(false),
            };
        }
        Ok(true)
    }
}

#[cfg(test)]
pub(crate) mod testing;
