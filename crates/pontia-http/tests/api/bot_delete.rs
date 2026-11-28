use super::{
    common::test_app::TestApp,
    support::http::{TOKEN, get_json, json_response},
};
use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use serde_json::Value;
use tower::ServiceExt;

async fn delete(app: &TestApp, bot: &str) -> (StatusCode, Value) {
    let response = crate::common::http::router(app.state.clone())
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/v1/bots/{bot}"))
                .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    json_response(response).await
}

#[tokio::test]
async fn an_open_helper_rejects_deletion_with_a_reason_until_it_exits() {
    let app = TestApp::new().await;
    sqlx::query(
        "INSERT INTO bots(bot_id,name,client_type,client_key,model) VALUES ('a','A','missing','test','model');
         INSERT INTO sessions(session_id,client_type,state,title) VALUES ('h','generic','idle','Fix the build');
         INSERT INTO bot_helpers(reference,bot_id,session_id) VALUES ('helper:a:1','a','h');
         INSERT INTO bot_messages(id,bot_id,kind,content) VALUES ('m','a','user_input','hi');",
    )
    .execute(&app.db)
    .await
    .unwrap();

    let (status, body) = get_json(app.state.clone(), "/api/v1/bots/a/deletion-blockers").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["running"], false);
    assert_eq!(body["data"]["helpers"][0]["session_id"], "h");

    let (status, body) = delete(&app, "a").await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "bot_delete_blocked");
    let (status, _) = get_json(app.state.clone(), "/api/v1/bots/a").await;
    assert_eq!(status, StatusCode::OK);

    sqlx::query("UPDATE sessions SET state='exited' WHERE session_id='h'")
        .execute(&app.db)
        .await
        .unwrap();
    let (status, body) = delete(&app, "a").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, _) = get_json(app.state.clone(), "/api/v1/bots/a").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = delete(&app, "a").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let helpers: i64 = sqlx::query_scalar("SELECT count(*) FROM bot_helpers")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(helpers, 0);
    let sessions: i64 = sqlx::query_scalar("SELECT count(*) FROM sessions WHERE session_id='h'")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(sessions, 1, "helper Session history remains");
}
