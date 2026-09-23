use crate::client_contract::{
    AgentClientSpec, ClientData, ClientLauncher, ClientSession, InProcessClient,
    TimelineBoundaryBackend, TurnTimelineBackend,
};
use std::{collections::HashMap, sync::Arc};

#[derive(Clone)]
pub struct ClientRegistration {
    pub in_process: Option<Arc<dyn InProcessClient>>,
    pub session: Option<Arc<dyn ClientSession>>,
    pub prepare_on_input: bool,
    pub steer: bool,
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
            entries: Arc::new(HashMap::new()),
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
        self.data(client).map(|data| data.timeline())
    }

    pub fn boundaries(&self, client: &str) -> Option<TimelineBoundaryBackend> {
        self.data(client).map(|data| data.boundaries())
    }
}
