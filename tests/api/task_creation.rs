#[path = "../support/http.rs"]
mod http;
#[path = "../support/task_state.rs"]
mod task_state;

use axum::http::StatusCode;
use http::{get_json, post_json};
use serde_json::json;
use task_state::test_state;

#[tokio::test]
async fn common_task_creation_endpoint_is_removed() {
    let state = test_state().await;

    let (status, _body) = post_json(
        state.clone(),
        "/external/v1/tasks",
        json!({"input":"legacy common task", "client_type":"generic"}),
    )
    .await;

    assert_eq!(status, StatusCode::GONE);

    let (status, body) = get_json(state, "/external/v1/tasks").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["tasks"].as_array().unwrap().len(), 0);
}
