use crate::client_contract::{
    AgentClientSpec, ClientData, ClientLauncher, ClientService, ClientServicePhase, ClientSession,
    InProcessClient, TimelineBoundaryBackend, TurnTimelineBackend,
};
use std::{collections::HashMap, sync::Arc};

#[derive(Clone)]
pub struct ClientRegistration {
    pub service: Option<Arc<dyn ClientService>>,
    pub in_process: Option<Arc<dyn InProcessClient>>,
    pub session: Option<Arc<dyn ClientSession>>,
    pub prepare_on_input: bool,
    pub steer: bool,
    pub spec: &'static AgentClientSpec,
    pub data: Option<Arc<dyn ClientData>>,
    pub launcher: Option<Arc<dyn ClientLauncher>>,
}

pub struct ClientServiceTasks {
    tasks: Vec<tokio::task::JoinHandle<pontia_core::Result<()>>>,
}

impl ClientServiceTasks {
    pub fn extend(&mut self, other: Self) {
        self.tasks.extend(other.tasks);
    }

    pub async fn join(self) -> pontia_core::Result<()> {
        let mut failure = None;
        for task in self.tasks {
            let result = task
                .await
                .map_err(|error| pontia_core::Error::Domain(error.to_string()))
                .and_then(|result| result);
            if let Err(error) = result
                && failure.is_none()
            {
                failure = Some(error);
            }
        }
        failure.map_or(Ok(()), Err)
    }

    async fn abort(self) {
        for task in &self.tasks {
            task.abort();
        }
        for task in self.tasks {
            let _ = task.await;
        }
    }
}

#[derive(Clone)]
pub struct ClientRegistry {
    entries: Arc<HashMap<&'static str, ClientRegistration>>,
    registration_order: Arc<Vec<&'static str>>,
}

impl Default for ClientRegistry {
    fn default() -> Self {
        Self {
            entries: Arc::new(HashMap::new()),
            registration_order: Arc::new(Vec::new()),
        }
    }
}

impl ClientRegistry {
    pub fn register(&mut self, client: ClientRegistration) {
        if !self.entries.contains_key(client.spec.client_type) {
            Arc::make_mut(&mut self.registration_order).push(client.spec.client_type);
        }
        Arc::make_mut(&mut self.entries).insert(client.spec.client_type, client);
    }

    pub async fn start_services(
        &self,
        state: crate::AppState,
        phase: ClientServicePhase,
    ) -> pontia_core::Result<ClientServiceTasks> {
        let mut tasks = ClientServiceTasks { tasks: Vec::new() };
        for client in self.registration_order.iter() {
            let Some(service) = self.entries[client]
                .service
                .as_ref()
                .filter(|service| service.phase() == phase)
            else {
                continue;
            };
            match service.start(state.clone()).await {
                Ok(Some(task)) => tasks.tasks.push(task),
                Ok(None) => {}
                Err(error) => {
                    tasks.abort().await;
                    return Err(error);
                }
            }
        }
        Ok(tasks)
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
