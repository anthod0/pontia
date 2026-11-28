use super::{
    common::test_app::TestApp,
    support::http::{TOKEN, get_json, json_response, post_json},
};
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use sha2::Digest;
use tower::ServiceExt;

fn configuration() -> Value {
    json!({"name":"  Atlas  ", "client_type":"pi", "client_key":"default", "model":"test"})
}

#[tokio::test]
async fn creating_a_bot_opens_an_idle_chat_in_the_requested_language() {
    let app = TestApp::new().await;
    for (locale, expected) in [
        (None, "Hi, I'm Atlas. What would you like me to help with?"),
        (
            Some("en"),
            "Hi, I'm Atlas. What would you like me to help with?",
        ),
        (
            Some("ja"),
            "こんにちは、Atlasです。何をお手伝いしましょうか？",
        ),
        (Some("zh-CN"), "你好，我是 Atlas。你想让我帮你做什么？"),
    ] {
        let mut config = configuration();
        config["description"] = json!("　 \t");
        if let Some(locale) = locale {
            config["locale"] = json!(locale);
        }
        let (status, body) = post_json(app.state.clone(), "/api/v1/bots", config).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let bot = &body["data"]["bot"];
        assert_eq!(bot["name"], "Atlas");
        assert!(bot["description"].is_null());
        assert!(bot["current_session_id"].is_null());
        let id = bot["bot_id"].as_str().unwrap();
        let (_, body) = get_json(app.state.clone(), &format!("/api/v1/bots/{id}")).await;
        let chat = &body["data"];
        assert_eq!(chat["status"]["kind"], "idle");
        let messages = chat["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0]["kind"], "bot_reply");
        assert_eq!(messages[0]["content"], expected);
    }
    for table in [
        "sessions",
        "bot_notifications",
        "bot_todos",
        "bot_message_links",
    ] {
        let count: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM {table}"))
            .fetch_one(&app.db)
            .await
            .unwrap();
        assert_eq!(count, 0, "{table}");
    }
    for (field, value) in [("locale", "fr"), ("language", "en")] {
        let mut config = configuration();
        config[field] = json!(value);
        assert_eq!(
            post_json(app.state.clone(), "/api/v1/bots", config).await.0,
            StatusCode::BAD_REQUEST
        );
    }
}

async fn set_description(
    router: &Router,
    session: &str,
    token: Option<&str>,
    description: &str,
) -> (StatusCode, Value) {
    let mut request =
        Request::post("/api/v1/bot/description").header("Content-Type", "application/json");
    if let Some(token) = token {
        request = request.header(pontia_bot::identity::TOOL_TOKEN_HEADER, token);
    }
    let input = json!({"session_id":session, "input":{"description":description}});
    json_response(
        router
            .clone()
            .oneshot(request.body(Body::from(input.to_string())).unwrap())
            .await
            .unwrap(),
    )
    .await
}

#[tokio::test]
async fn only_the_current_bot_can_fill_its_empty_description() {
    let app = TestApp::new().await;
    let router = crate::common::http::router(app.state.clone());
    let long = "文".repeat(2001);
    let mut config = configuration();
    config["description"] = json!(&long);
    let (status, body) = post_json(app.state.clone(), "/api/v1/bots", config.clone()).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let bot = &body["data"]["bot"];
    assert_eq!(bot["description"], long);
    let id = bot["bot_id"].as_str().unwrap();
    for (session, owner) in [
        ("current", format!("{id}:current")),
        ("old", format!("{id}:old")),
        ("helper", format!("helper:{id}:1")),
        ("dream", format!("dreaming:{id}")),
    ] {
        sqlx::query("INSERT INTO sessions(session_id, client_type, state, application_id, application_ref, credential_env, credential_sha256) VALUES (?, 'pi', 'idle', 'bot', ?, 'PONTIA_BOT_TOOL_TOKEN', ?)")
            .bind(session).bind(owner).bind(sha2::Sha256::digest(session).to_vec())
            .execute(&app.db).await.unwrap();
    }
    sqlx::query("UPDATE bots SET current_session_id = 'current' WHERE bot_id = ?")
        .bind(id)
        .execute(&app.db)
        .await
        .unwrap();
    for (session, token) in [
        ("current", None),
        ("current", Some("old")),
        ("old", Some("old")),
        ("helper", Some("helper")),
        ("dream", Some("dream")),
    ] {
        assert_eq!(
            set_description(&router, session, token, "test").await.0,
            StatusCode::FORBIDDEN
        );
    }
    let admin_only = Request::post("/api/v1/bot/description")
        .header("Authorization", format!("Bearer {TOKEN}"))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({"session_id":"current","input":{"description":"test"}}).to_string(),
        ))
        .unwrap();
    assert_eq!(
        router.clone().oneshot(admin_only).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
    for bearer in [None, Some("current"), Some(TOKEN)] {
        let mut request =
            Request::get("/api/v1/bots").header(pontia_bot::identity::TOOL_TOKEN_HEADER, "current");
        if let Some(token) = bearer {
            request = request.header("Authorization", format!("Bearer {token}"));
        }
        let response = router
            .clone()
            .oneshot(request.body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
    let set = |text| set_description(&router, "current", Some("current"), text);
    let (status, body) = set("test").await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "bot_description_already_set");

    for description in [&long[..], " \t　"] {
        config["description"] = json!(description);
        let request = Request::put(format!("/api/v1/bots/{id}"))
            .header("Authorization", format!("Bearer {TOKEN}"))
            .header("Content-Type", "application/json")
            .body(Body::from(config.to_string()))
            .unwrap();
        let (status, body) = json_response(router.clone().oneshot(request).await.unwrap()).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(
            body["data"]["bot"]["description"],
            if description.trim().is_empty() {
                Value::Null
            } else {
                json!(description)
            }
        );
    }
    assert_eq!(set("　 ").await.0, StatusCode::BAD_REQUEST);
    assert_eq!(set(&long).await.0, StatusCode::BAD_REQUEST);
    let max = "文".repeat(2000);
    let (one, two) = tokio::join!(set(&max), set(&max));
    let (success, conflict) = if one.0 == StatusCode::OK {
        (one, two)
    } else {
        (two, one)
    };
    assert_eq!(success.0, StatusCode::OK);
    assert_eq!(success.1["data"], json!({"description":max}));
    assert_eq!(conflict.0, StatusCode::CONFLICT);
    assert_eq!(conflict.1["error"]["code"], "bot_description_already_set");
    for legacy in ["", " \t\n"] {
        sqlx::query("UPDATE bots SET description = ? WHERE bot_id = ?")
            .bind(legacy)
            .bind(id)
            .execute(&app.db)
            .await
            .unwrap();
        let (status, body) = set("  New role  ").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["data"], json!({"description":"New role"}));
    }
}
