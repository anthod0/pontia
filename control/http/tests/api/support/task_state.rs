use pontia_application::AppState;

use crate::common::test_app::TestApp;

use super::http::TOKEN;

pub(crate) async fn test_state() -> AppState {
    TestApp::builder()
        .database_name("global_workspace_tasks.db")
        .external_api_token(Some(TOKEN.to_string()))
        .build_state()
        .await
}
