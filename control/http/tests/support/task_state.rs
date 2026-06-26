use pontia_application::AppState;

use super::{http::TOKEN, test_app::TestApp};

pub async fn test_state() -> AppState {
    TestApp::builder()
        .database_name("global_workspace_tasks.db")
        .external_api_token(Some(TOKEN.to_string()))
        .build_state()
        .await
}
