use std::{collections::HashMap, path::Path, sync::Arc};

use pontia_agent_clients::{
    AgentClientSpec, TimelineBoundaryBackend, TurnTimelineBackend, TurnTopologyBackend,
};
use pontia_core::{
    Result,
    domain::{DomainEvent, EventType},
};
use pontia_runtime::{RuntimeStartRequest, RuntimeStartResult};
use serde_json::Value;

#[derive(Default)]
pub struct NativeEventEvidence {
    pub entry_anchor: Option<String>,
    pub topology: Option<Value>,
}

pub struct BranchTargetRequest {
    pub binding: pontia_storage_sqlite::models::agent_bindings::AgentBindingRow,
    pub turn: pontia_storage_sqlite::models::turns::TurnProjectionRow,
    pub is_first_session_turn: bool,
}

/// Native parsing operates on opaque client evidence; business identity stays with services.
pub trait ClientData: Send + Sync {
    fn normalize_payload(&self, kind: EventType, data: Value) -> Result<Value>;
    fn take_evidence(&self, event: &mut DomainEvent) -> NativeEventEvidence;
    fn timeline(&self) -> TurnTimelineBackend;
    fn boundaries(&self) -> TimelineBoundaryBackend;
    fn topology(&self) -> Option<TurnTopologyBackend>;
    fn branch_target(&self, request: BranchTargetRequest) -> Result<String>;
}

pub struct ClientLaunchRequest<'a> {
    pub root: &'a Path,
    pub runtime: RuntimeStartRequest,
    pub restart_count: i64,
    pub reuse_pane: Option<(&'a str, &'a str)>,
    pub native_session_key: Option<&'a str>,
}

pub trait ClientLauncher: Send + Sync {
    fn launch(&self, request: ClientLaunchRequest<'_>) -> Result<RuntimeStartResult>;
}

#[derive(Clone)]
pub struct ClientRegistration {
    pub spec: &'static AgentClientSpec,
    pub data: Option<Arc<dyn ClientData>>,
    pub launcher: Option<Arc<dyn ClientLauncher>>,
}

#[derive(Clone)]
pub struct ClientRegistry {
    entries: Arc<HashMap<&'static str, ClientRegistration>>,
}

impl Default for ClientRegistry {
    fn default() -> Self {
        Self {
            entries: Arc::new(
                pontia_agent_clients::AGENT_CLIENTS
                    .iter()
                    .map(|spec| {
                        (
                            spec.client_type,
                            ClientRegistration {
                                spec,
                                data: None,
                                launcher: None,
                            },
                        )
                    })
                    .collect(),
            ),
        }
    }
}

impl ClientRegistry {
    pub fn register(&mut self, client: ClientRegistration) {
        Arc::make_mut(&mut self.entries).insert(client.spec.client_type, client);
    }

    pub fn get(&self, client: &str) -> Option<&ClientRegistration> {
        self.entries.get(client)
    }

    pub fn spec(&self, client: &str) -> Option<&'static AgentClientSpec> {
        self.get(client).map(|entry| entry.spec)
    }

    pub fn data(&self, client: &str) -> Option<&Arc<dyn ClientData>> {
        self.get(client).and_then(|entry| entry.data.as_ref())
    }

    pub fn timeline(&self, client: &str) -> Option<TurnTimelineBackend> {
        self.data(client)
            .map(|data| data.timeline())
            .or_else(|| pontia_agent_clients::turn_timeline_backend_for(client))
    }

    pub fn boundaries(&self, client: &str) -> Option<TimelineBoundaryBackend> {
        self.data(client)
            .map(|data| data.boundaries())
            .or_else(|| pontia_agent_clients::timeline_boundary_backend_for(client))
    }
}
