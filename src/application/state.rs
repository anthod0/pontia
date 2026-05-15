use super::*;
use crate::{
    application::set_default_client_type, runtime::set_runtime_config,
    transport::http::dashboard::ResolvedDashboard,
};

#[derive(Clone)]
pub struct AppState {
    pub db: sqlx::SqlitePool,
    pub external_api_token: Option<String>,
    pub graph: GraphRuntimeConfig,
    pub workspace_browser: WorkspaceBrowserConfig,
    pub dashboard: ResolvedDashboard,
}

pub async fn initialize(config: &AppConfig) -> Result<AppState> {
    let db = connect_sqlite(&config.database_url).await?;

    if config.run_migrations {
        run_migrations(&db).await?;
    }

    set_default_client_type(config.default_client_type.clone());
    set_runtime_config(config.runtime.clone());
    let dashboard = crate::transport::http::dashboard::resolve_dashboard(&config.dashboard).await;

    Ok(AppState {
        db,
        external_api_token: config.external_api_token.clone(),
        graph: config.graph.clone(),
        workspace_browser: config.workspace_browser.clone(),
        dashboard,
    })
}
