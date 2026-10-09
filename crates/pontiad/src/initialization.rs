use pontia_config::AppConfig;
use pontia_core::error::Result;
use pontia_storage_sqlite::{connect_sqlite, run_migrations};

use pontia_application::{AppState, app::set_default_client_type};

pub async fn initialize(config: &AppConfig) -> Result<AppState> {
    let clients = pontia_clients::registration(config);
    if clients.spec(&config.default_client_type).is_none() {
        return Err(pontia_core::Error::InvalidConfig {
            key: "PONTIA_DEFAULT_CLIENT_TYPE",
            message: format!(
                "client {} is currently unavailable (not registered)",
                config.default_client_type
            ),
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
    state.inbox_commands().recover_deliveries().await?;
    Ok(state)
}
