use pontia_application::clients::ClientRegistry;
use pontia_config::AppConfig;

/// The composition root for the Agent Clients included in Pontia.
pub fn registration(config: &AppConfig) -> ClientRegistry {
    let mut clients = ClientRegistry::default();
    clients.register(pontia_client_pi::registration(
        config.runtime.tui_command_for_client_config_key("pi"),
    ));
    clients.register(pontia_client_codex::registration());
    clients
}
