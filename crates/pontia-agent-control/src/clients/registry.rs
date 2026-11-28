use crate::client_contract::install::ClientInstaller;
use crate::client_contract::{
    AgentClientSpec, ClientData, ClientService, ClientServicePhase, ClientSession,
    TimelineBoundaryBackend, TurnTimelineBackend,
};
use pontia_storage_sqlite::repositories::agent_clients::SqliteAgentClientRepository;
use sqlx::SqlitePool;
use std::{collections::HashMap, sync::Arc};

#[derive(Clone)]
pub struct ClientRegistration {
    pub configuration: Option<Arc<dyn crate::client_contract::ClientConfiguration>>,
    pub auth: Option<Arc<dyn crate::client_contract::ClientAuth>>,
    pub interactions: Option<Arc<dyn crate::client_contract::ClientInteractions>>,
    pub events: Option<Arc<dyn crate::client_contract::ClientEventInterpreter>>,
    pub service: Option<Arc<dyn ClientService>>,
    pub instructions: Option<Arc<dyn crate::client_contract::ClientInstructions>>,
    pub session: Option<Arc<dyn ClientSession>>,
    pub prepare_on_input: bool,
    pub spec: &'static AgentClientSpec,
    pub data: Option<Arc<dyn ClientData>>,
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

type Installers = HashMap<(&'static str, &'static str), Arc<dyn ClientInstaller>>;

#[derive(Clone)]
pub struct ClientRegistry {
    installers: Arc<Installers>,
    entries: Arc<HashMap<&'static str, ClientRegistration>>,
    registration_order: Arc<Vec<&'static str>>,
    integrations: Arc<HashMap<&'static str, Arc<dyn crate::client_contract::ClientIntegration>>>,
}

impl Default for ClientRegistry {
    fn default() -> Self {
        Self {
            installers: Arc::default(),
            entries: Arc::new(HashMap::new()),
            registration_order: Arc::new(Vec::new()),
            integrations: Arc::new(HashMap::new()),
        }
    }
}

impl ClientRegistry {
    pub fn register_installer(&mut self, installer: Arc<dyn ClientInstaller>) {
        Arc::make_mut(&mut self.installers)
            .insert((installer.client_type(), installer.client_key()), installer);
    }

    pub fn installer(&self, client: &str, key: &str) -> Option<Arc<dyn ClientInstaller>> {
        self.installers.get(&(client, key)).cloned()
    }

    pub fn register_integration(
        &mut self,
        integration: Arc<dyn crate::client_contract::ClientIntegration>,
    ) {
        Arc::make_mut(&mut self.integrations).insert(integration.client_type(), integration);
    }

    pub fn integration(
        &self,
        client: &str,
        key: &str,
    ) -> Option<&Arc<dyn crate::client_contract::ClientIntegration>> {
        self.integrations
            .get(client)
            .filter(|integration| integration.client_key() == key)
    }

    pub fn register(&mut self, client: ClientRegistration) {
        if !self.entries.contains_key(client.spec.client_type) {
            Arc::make_mut(&mut self.registration_order).push(client.spec.client_type);
        }
        Arc::make_mut(&mut self.entries).insert(client.spec.client_type, client);
    }

    pub async fn start_services(
        &self,
        state: crate::AgentControl,
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
        if phase == ClientServicePhase::Transport
            && self.registrations().any(|registration| {
                registration
                    .service
                    .as_ref()
                    .is_some_and(|service| service.uses_control_socket())
            })
        {
            let listener = match crate::ipc::ControlListener::bind(state.pontia_home()).await {
                Ok(listener) => listener,
                Err(error) => {
                    tasks.abort().await;
                    return Err(error);
                }
            };
            let shutdown = state.shutdown().subscribe();
            tasks
                .tasks
                .push(tokio::spawn(listener.run(state, shutdown)));
        }
        Ok(tasks)
    }

    pub async fn creation_options(
        &self,
        pool: &SqlitePool,
    ) -> pontia_core::Result<Vec<crate::client_contract::SessionCreationOption>> {
        let enabled = SqliteAgentClientRepository::new(pool.clone())
            .list()
            .await?
            .into_iter()
            .filter(|row| row.enabled)
            .map(|row| ((row.client_type, row.client_key), row.version))
            .collect::<HashMap<_, _>>();
        let mut options = Vec::new();
        for registration in self.registrations() {
            let client = registration.spec.client_type;
            if let Some(session) = &registration.session
                && let Some(client_options) = session.creation_options().await?
            {
                for option in client_options {
                    let key = match &registration.configuration {
                        Some(configuration) => configuration.key(&option.metadata)?,
                        None => "default".into(),
                    };
                    if enabled.contains_key(&(client.into(), key)) {
                        options.push(option);
                    }
                }
                continue;
            }
            let key = match &registration.configuration {
                Some(configuration) => configuration.key(&serde_json::json!({}))?,
                None => "default".into(),
            };
            let Some(version) = enabled.get(&(client.into(), key.clone())) else {
                continue;
            };
            let name = registration
                .configuration
                .as_ref()
                .and_then(|configuration| {
                    let key = configuration.keys().into_iter().next()?;
                    let effective = configuration.merged(&key);
                    Some(configuration.presentation(&key, &effective).0)
                })
                .unwrap_or_else(|| client.into());
            options.push(crate::client_contract::SessionCreationOption {
                model_catalog_key: registration
                    .configuration
                    .as_ref()
                    .filter(|c| c.supports_startup_model())
                    .map(|_| key),
                id: client.into(),
                name,
                client_type: client.into(),
                metadata: serde_json::json!({}),
                version: version.clone(),
            });
        }
        Ok(options)
    }

    pub async fn creation_metadata(
        &self,
        pool: &sqlx::SqlitePool,
        client: &str,
        key: &str,
    ) -> pontia_core::Result<serde_json::Value> {
        let entry = self
            .get(client)
            .ok_or_else(|| pontia_core::Error::Domain(format!("Unknown client: {client}")))?;
        let configuration = entry.configuration.as_ref().ok_or_else(|| {
            pontia_core::Error::Unsupported(format!("Client configuration unavailable: {client}"))
        })?;
        if !configuration.keys().iter().any(|known| known == key) {
            return Err(pontia_core::Error::Domain(format!(
                "Unknown client configuration: {client}/{key}"
            )));
        }
        for option in self.creation_options(pool).await? {
            if option.client_type == client && configuration.key(&option.metadata)? == key {
                return Ok(option.metadata);
            }
        }
        Err(pontia_core::Error::Conflict {
            code: "client_disabled",
            message: format!("Client {client}/{key} is unavailable for Session creation"),
        })
    }

    pub fn default_client_type(&self) -> pontia_core::Result<&'static str> {
        self.registrations()
            .find(|client| client.spec.default_for_creation)
            .map(|client| client.spec.client_type)
            .ok_or_else(|| {
                pontia_core::Error::Unsupported("No default Agent Client is registered".into())
            })
    }

    pub fn registrations(&self) -> impl Iterator<Item = &ClientRegistration> {
        self.registration_order
            .iter()
            .map(|client| &self.entries[client])
    }

    pub fn get(&self, client: &str) -> Option<&ClientRegistration> {
        self.entries.get(client)
    }

    pub fn spec(&self, client: &str) -> Option<&'static AgentClientSpec> {
        self.get(client).map(|entry| entry.spec)
    }

    pub fn control_runtime_role(&self, client: &str) -> Option<&'static str> {
        self.spec(client)
            .and_then(|spec| spec.adapter.control_runtime_role)
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

#[cfg(test)]
mod install_tests {
    use super::*;
    use crate::client_contract::{DetectionContext, install::InstallSpec};
    use pontia_runtime::client_operations::InstallOperation;

    struct Installer(&'static str);

    impl ClientInstaller for Installer {
        fn client_type(&self) -> &'static str {
            "test"
        }
        fn client_key(&self) -> &'static str {
            self.0
        }
        fn inspect(&self, _: &DetectionContext<'_>) -> InstallSpec {
            unreachable!()
        }
        fn install(&self, _: &str, _: &mut InstallOperation) -> std::io::Result<()> {
            unreachable!()
        }
    }

    #[test]
    fn installers_are_registered_per_client_type_and_key() {
        let mut registry = ClientRegistry::default();
        for key in ["first", "second"] {
            registry.register_installer(Arc::new(Installer(key)));
        }
        for key in ["first", "second"] {
            assert_eq!(registry.installer("test", key).unwrap().client_key(), key);
        }
        assert!(registry.installer("other", "first").is_none());
        assert!(registry.installer("test", "missing").is_none());
    }
}
