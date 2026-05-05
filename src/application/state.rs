use super::*;

#[derive(Clone)]
pub struct AppState {
    pub db: sqlx::SqlitePool,
    pub external_api_token: Option<String>,
    pub planner: PlannerRuntimeConfig,
}

pub async fn initialize(config: &AppConfig) -> Result<AppState> {
    let db = connect_sqlite(&config.database_url).await?;

    if config.run_migrations {
        run_migrations(&db).await?;
    }

    Ok(AppState {
        db,
        external_api_token: config.external_api_token.clone(),
        planner: config.planner.clone(),
    })
}
