use pontia_agent_control::{client_contract::ClientIntegration, clients::ClientRegistry};
pub use pontia_client_acp::relay::run_from_stdin as run_stdio_relay;
use std::sync::Arc;

pub fn integrations() -> Vec<Arc<dyn ClientIntegration>> {
    integrations_for_version(pontia_version::version())
}

pub fn integrations_for_version(version: &str) -> Vec<Arc<dyn ClientIntegration>> {
    vec![
        pontia_client_pi::setup::integration(),
        pontia_client_codex::setup::integration(),
        pontia_client_claude::setup::integration(version),
    ]
}

pub fn service_path_variables() -> Vec<&'static str> {
    integrations()
        .iter()
        .flat_map(|client| client.service_path_variables().iter().copied())
        .collect()
}
/// The composition root for the Agent Clients included in Pontia.
pub fn registration() -> ClientRegistry {
    let mut clients = ClientRegistry::default();
    clients.register_installer(Arc::new(pontia_client_pi::config::PiConfiguration));
    clients.register_installer(Arc::new(pontia_client_codex::install::CodexInstaller));
    clients.register(pontia_client_pi::registration());
    clients.register(pontia_client_codex::registration());
    clients.register(pontia_client_claude::registration());
    clients.register(pontia_client_acp::registration());
    clients.register_installer(Arc::new(pontia_client_claude::install::ClaudeInstaller));
    for installer in pontia_client_acp::install::installers() {
        clients.register_installer(installer);
    }
    for integration in integrations() {
        clients.register_integration(integration);
    }
    clients
}
