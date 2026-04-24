use crate::{
    config::AppConfig,
    error::Result,
    storage::sqlite::{connect_sqlite, run_migrations},
};

#[derive(Clone)]
pub struct AppState {
    pub db: sqlx::SqlitePool,
}

pub async fn initialize(config: &AppConfig) -> Result<AppState> {
    let db = connect_sqlite(&config.database_url).await?;

    if config.run_migrations {
        run_migrations(&db).await?;
    }

    Ok(AppState { db })
}
