use pontia_config::AppConfig;
use pontia_core::error::Result;
use pontia_runtime::{set_runtime_bind_addr, set_runtime_config};
use pontia_storage_sqlite::{connect_sqlite, run_migrations};

use super::{AppState, set_default_client_type};

pub async fn initialize(config: &AppConfig) -> Result<AppState> {
    let db = connect_sqlite(&config.database_url).await?;

    if config.run_migrations {
        run_migrations(&db).await?;
    }

    set_default_client_type(config.default_client_type.clone());
    set_runtime_config(config.runtime.clone());
    set_runtime_bind_addr(config.bind_addr);
    Ok(AppState::builder(db, config.pontia_home.clone())
        .external_api_token(config.external_api_token.clone())
        .workspace_browser(config.workspace_browser.clone())
        .file_picker(config.file_picker.clone())
        .build())
}
