use pontia::application::AppState;
use pontia_storage_sqlite::{connect_sqlite, run_migrations};

use super::http::TOKEN;

pub async fn test_state() -> AppState {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("global_workspace_tasks.db");
    let _kept_dir = dir.keep();
    let database_url = format!("sqlite://{}", db_path.display());
    let db = connect_sqlite(&database_url).await.expect("connect");
    run_migrations(&db).await.expect("migrate");
    AppState::builder(db)
        .external_api_token(Some(TOKEN.to_string()))
        .build()
}
