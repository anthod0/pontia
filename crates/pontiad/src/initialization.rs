use pontia_config::AppConfig;
use pontia_core::error::Result;
use pontia_storage_sqlite::{connect_sqlite, run_migrations};

use pontia_application::{AppState, app::set_default_client_type};

pub async fn initialize(config: &AppConfig) -> Result<AppState> {
    let mut clients = pontia_application::clients::ClientRegistry::default();
    clients.register(pontia_client_pi::registration(
        config.runtime.tui_command_for_client_config_key("pi"),
    ));
    clients.register(pontia_client_codex::registration());
    if clients.spec(&config.default_client_type).is_none() {
        return Err(pontia_core::Error::InvalidConfig {
            key: "PONTIA_DEFAULT_CLIENT_TYPE",
            message: "default client is not registered".into(),
        });
    }
    let db = connect_sqlite(&config.database_url).await?;

    if config.run_migrations {
        run_migrations(&db).await?;
    }

    set_default_client_type(config.default_client_type.clone());
    let state = AppState::builder(db, config.pontia_home.clone())
        .clients(clients)
        .external_api_token(config.external_api_token.clone())
        .workspace_browser(config.workspace_browser.clone())
        .file_picker(config.file_picker.clone())
        .build();
    pontia_client_codex::CodexService::new(state.event_ingest_service())
        .reset_connections()
        .await?;
    state.inbox_commands().recover_deliveries().await?;
    Ok(state)
}
