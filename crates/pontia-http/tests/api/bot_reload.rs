use super::{common::test_app::TestApp, support::http::post_json};
use axum::http::StatusCode;
use serde_json::Value;

#[tokio::test]
async fn reload_reports_busy_and_accepts_an_exited_session() {
    let app = TestApp::new().await;
    sqlx::query("INSERT INTO sessions(session_id,client_type,state) VALUES ('s','generic','busy');
        INSERT INTO bots(bot_id,name,client_type,client_key,model,current_session_id) VALUES ('a','A','generic','test','model','s');")
        .execute(&app.db).await.unwrap();
    let path = "/api/v1/bots/a/reload";
    let (status, body) = post_json(app.state.clone(), path, Value::Null).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "state_conflict");
    sqlx::query("UPDATE sessions SET state='exited' WHERE session_id='s'")
        .execute(&app.db)
        .await
        .unwrap();
    let (status, body) = post_json(app.state.clone(), path, Value::Null).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let current: Option<String> =
        sqlx::query_scalar("SELECT current_session_id FROM bots WHERE bot_id='a'")
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(current, None);
}
