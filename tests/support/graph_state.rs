use llmparty::{
    application::{AppState, GraphRuntimeConfig, PlannerRuntimeConfig},
    storage::sqlite::{connect_sqlite, run_migrations},
};

use super::http::TOKEN;

pub async fn graph_planner_test_state(graph_dir: String) -> AppState {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("global_workspace_tasks.db");
    let _kept_dir = dir.keep();
    let database_url = format!("sqlite://{}", db_path.display());
    let db = connect_sqlite(&database_url).await.expect("connect");
    run_migrations(&db).await.expect("migrate");
    AppState {
        db,
        external_api_token: Some(TOKEN.to_string()),
        planner: PlannerRuntimeConfig {
            enabled: true,
            client_type: "generic".to_string(),
            timeout_ms: 30_000,
            compatibility_direct_dispatch: false,
        },
        graph: GraphRuntimeConfig {
            enabled: true,
            db_dir: Some(graph_dir),
        },
        workspace_browser: Default::default(),
    }
}
