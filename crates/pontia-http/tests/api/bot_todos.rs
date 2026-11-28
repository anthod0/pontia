use super::{
    common::test_app::TestApp,
    support::http::{TOKEN, json_response, post_json},
};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::json;
use tower::ServiceExt;

#[tokio::test]
async fn dashboard_todos_are_scoped_idempotent_and_only_accept_normal_messages() {
    let app = TestApp::new().await;
    let (_, body) = post_json(
        app.state.clone(),
        "/api/v1/bots",
        json!({"name":"Planner","client_type":"pi","client_key":"default","model":"test"}),
    )
    .await;
    let bot = body["data"]["bot"]["bot_id"].as_str().unwrap();
    let router = crate::common::http::router(app.state.clone());
    sqlx::query("INSERT INTO bot_messages(id,bot_id,kind,status,content) VALUES ('user',?,'user_input','normal','Review'),('failed',?,'user_input','failed','Retry')")
        .bind(bot).bind(bot).execute(&app.db).await.unwrap();
    let change = |bot: &str, id: &str, method| {
        let request = Request::builder()
            .method(method)
            .uri(format!("/api/v1/bots/{bot}/messages/{id}/todo"))
            .header("Authorization", format!("Bearer {TOKEN}"))
            .body(Body::empty())
            .unwrap();
        let router = router.clone();
        async move { json_response(router.oneshot(request).await.unwrap()).await }
    };
    assert_eq!(
        change("other", "user", "PUT").await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(change(bot, "failed", "PUT").await.0, StatusCode::CONFLICT);
    let (status, body) = change(bot, "user", "DELETE").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["data"]["message"]["todo"].is_null());
    for (method, state) in [("PUT", "open"), ("DELETE", "done"), ("PUT", "open")] {
        let (status, body) = change(bot, "user", method).await;
        assert_eq!(status, StatusCode::OK);
        let todo = &body["data"]["message"]["todo"];
        assert_eq!(todo["state"], state);
        assert_eq!(todo["by"], "user");
        assert_eq!(change(bot, "user", method).await.1, body);
    }
}
