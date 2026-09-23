mod input;
mod lifecycle;
mod models;
pub(crate) use lifecycle::discard_unbound_runtime;
mod channel;
mod registry;
pub use registry::*;

use crate::{ClientControlService, EventIngestService};
use pontia_agent_clients::{AgentClientSpec, DispatchMode};
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

    pub fn supports_steer(&self) -> bool {
        self.spec.adapter.dispatch == DispatchMode::CodexProtocol
    }
    pub fn prepares_on_input(&self) -> bool {
        self.spec.adapter.dispatch == DispatchMode::CodexProtocol
    }
    pub fn client_owns_turn(&self) -> bool {
        self.prepares_on_input() || self.spec.owns_interactive_tmux_turn()
    }
    pub fn supports_restart(&self) -> bool {
        self.spec.adapter.runtime != pontia_agent_clients::RuntimeBehavior::CodexAppServer
    }

    pub async fn input_available(&self, session: &str) -> Result<bool> {
        if self.prepares_on_input() {
            let state: Option<String> = sqlx::query_scalar("SELECT json_extract(adapter_details,'$.codex.connection') FROM runtime_bindings WHERE session_id=?")
                .bind(session).fetch_optional(&self.events.db()).await?.flatten();
            return Ok(matches!(
                state.as_deref(),
                Some("available" | "awaiting_input")
            ));
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
