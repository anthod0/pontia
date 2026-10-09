use pontia_application::{client_contract::ClientIntegration, clients::ClientRegistry};
use std::sync::Arc;

pub fn integrations() -> Vec<Arc<dyn ClientIntegration>> {
    vec![
        pontia_client_pi::setup::integration(),
        pontia_client_codex::setup::integration(),
    ]
}

pub fn service_path_variables() -> Vec<&'static str> {
    integrations()
        .iter()
        .flat_map(|client| client.service_path_variables().iter().copied())
        .collect()
}
use pontia_config::AppConfig;

pub fn config_from_env() -> pontia_core::Result<AppConfig> {
    AppConfig::from_env(&pontia_client_pi::config::DEFAULTS)
}

pub fn config_from_vars(
    vars: &std::collections::HashMap<String, String>,
) -> pontia_core::Result<AppConfig> {
    AppConfig::from_vars(vars, &pontia_client_pi::config::DEFAULTS)
}

/// The composition root for the Agent Clients included in Pontia.
pub fn registration(config: &AppConfig) -> ClientRegistry {
    let mut clients = ClientRegistry::default();
    clients.register(pontia_client_pi::registration(
        config.runtime.tui_command_for_client_config_key("pi"),
    ));
    clients.register(pontia_client_codex::registration());
    clients
}
