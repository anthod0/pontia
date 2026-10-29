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
    let sources = &config.client_configuration;
    let mut pi = pontia_client_pi::registration(None);
    pi.configuration = Some(Arc::new(pontia_client_pi::config::PiConfiguration::new(
        sources
            .runtime_file
            .get("pi")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({})),
        sources
            .runtime_environment
            .get("pi")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({})),
    )));
    clients.register(pi);
    let mut codex = pontia_client_codex::registration();
    codex.configuration = Some(Arc::new(
        pontia_client_codex::configuration::CodexConfiguration::new(
            sources
                .runtime_file
                .get("codex")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({})),
        ),
    ));
    clients.register(codex);
    clients.register(pontia_client_claude::registration());
    clients.register(pontia_client_acp::registration_from_sources(
        sources.acp_file.clone(),
    ));
    clients
}
