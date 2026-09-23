mod input;
mod lifecycle;
pub(crate) use lifecycle::discard_unbound_runtime;
mod pi;

use crate::{EventIngestService, PiControlService};
use pontia_agent_clients::{AgentClientSpec, DispatchMode, get_client_spec};
use pontia_core::{Error, Result};

/// Client selection and native execution live here. Business policy stays with
/// the service that owns the operation.
pub(crate) struct ClientAdapter {
    pub spec: &'static AgentClientSpec,
    pub events: EventIngestService,
    pub pi: Option<PiControlService>,
}

impl ClientAdapter {
    pub fn new(
        client: &str,
        events: EventIngestService,
        pi: Option<PiControlService>,
    ) -> Result<Self> {
        Ok(Self {
            spec: get_client_spec(client)
                .ok_or_else(|| Error::Domain(format!("unsupported client_type: {client}")))?,
            events,
            pi,
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
        if self.spec.adapter.dispatch == DispatchMode::PiControl {
            return Ok(self.pi.is_some() && pontia_storage_sqlite::repositories::runtime_bindings::SqliteRuntimeBindingRepository::new(self.events.db()).pi_control_endpoint(session).await?.is_some());
        }
        Ok(true)
    }
}
