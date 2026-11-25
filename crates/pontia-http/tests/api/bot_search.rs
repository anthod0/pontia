use super::{common::test_app::TestApp, support::http::get_json};
use axum::http::StatusCode;
use serde_json::{Value, json};

async fn fixture() -> TestApp {
    let app = TestApp::new().await;
    sqlx::query(
        "INSERT INTO bots(bot_id,name,client_type,client_key,model)
        VALUES ('a','A','missing','test','model'), ('b','B','missing','test','model')",
    )
    .execute(&app.db)
    .await
    .unwrap();
    for (bot, id, kind, time, content) in [
        (
            "a",
            "m1",
            "user_input",
            "2026-11-20T00:00:01Z",
            "部署失败了，帮我看看",
        ),
        (
            "a",
            "m2",
            "bot_reply",
            "2026-11-20T00:00:02Z",
            "部署日志显示磁盘已满",
        ),
        (
            "a",
            "m3",
            "user_input",
            "2026-11-20T00:00:03Z",
            "进度 100% 完成",
        ),
        (
            "a",
            "m4",
            "user_input",
            "2026-11-20T00:00:04Z",
            "100 percent",
        ),
        (
            "a",
            "m5",
            "bot_reply",
            "2026-11-20T00:00:05Z",
            "file_name and filename",
        ),
        (
            "a",
            "m6",
            "bot_reply",
            "2026-11-20T00:00:06Z",
            r"path C:\temp and Ctemp",
        ),
        (
            "a",
            "m7",
            "user_input",
            "2026-11-20T00:00:07Z",
            "Deploy again",
        ),
        (
            "a",
            "t1",
            "user_input",
            "2026-11-20T00:00:09.000Z",
            "tied 平局",
        ),
        ("a", "t3", "bot_reply", "2026-11-20T00:00:09Z", "tied 平局"),
        (
            "a",
            "t2",
            "user_input",
            "2026-11-20T08:00:09+08:00",
            "tied 平局",
        ),
        (
            "b",
            "other",
            "user_input",
            "2026-11-20T00:00:08Z",
            "部署 elsewhere",
        ),
    ] {
        sqlx::query(
            "INSERT INTO bot_messages(bot_id,id,kind,created_at,content) VALUES (?,?,?,?,?)",
        )
        .bind(bot)
        .bind(id)
        .bind(kind)
        .bind(time)
        .bind(content)
        .execute(&app.db)
        .await
        .unwrap();
    }
    app
}

async fn search(app: &TestApp, bot: &str, query: &[(&str, &str)]) -> (StatusCode, Value) {
    let mut url = reqwest::Url::parse(&format!(
        "http://localhost/api/v1/bots/{bot}/messages/search"
    ))
    .unwrap();
    url.query_pairs_mut().extend_pairs(query.iter().copied());
    get_json(app.state.clone(), &url.as_str()["http://localhost".len()..]).await
}

async fn ids(app: &TestApp, query: &[(&str, &str)]) -> (Vec<String>, Value) {
    let (status, body) = search(app, "a", query).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let data = body["data"].clone();
    let ids = data["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|message| message["id"].as_str().unwrap().to_owned())
        .collect();
    (ids, data["next_before"].clone())
}

#[tokio::test]
async fn matches_chinese_substrings_in_one_bot_newest_first() {
    let app = fixture().await;
    let (found, next) = ids(&app, &[("q", "部署")]).await;
    assert_eq!(found, ["m2", "m1"]);
    assert!(next.is_null());
    let (status, body) = search(&app, "a", &[("q", "部署")]).await;
    assert_eq!(status, StatusCode::OK);
    let message = &body["data"]["messages"][0];
    assert_eq!(message["kind"], "bot_reply");
    assert_eq!(message["created_at"], "2026-11-20T00:00:02Z");
    assert_eq!(ids(&app, &[("q", "磁盘已满")]).await.0, ["m2"]);
    assert_eq!(ids(&app, &[("q", "deploy")]).await.0, ["m7"]);
}

#[tokio::test]
async fn treats_like_wildcards_and_escape_characters_as_text() {
    let app = fixture().await;
    assert_eq!(ids(&app, &[("q", "100%")]).await.0, ["m3"]);
    assert_eq!(ids(&app, &[("q", "file_name")]).await.0, ["m5"]);
    assert_eq!(ids(&app, &[("q", r"C:\temp")]).await.0, ["m6"]);
    assert_eq!(ids(&app, &[("q", "%")]).await.0, ["m3"]);
    assert!(
        ids(&app, &[("q", "'; DROP TABLE bot_messages; --")])
            .await
            .0
            .is_empty()
    );
}

#[tokio::test]
async fn pages_with_the_last_message_id_through_tied_times() {
    let app = fixture().await;
    let mut seen = Vec::new();
    let (mut page, mut next) = ids(&app, &[("q", "平局"), ("limit", "1")]).await;
    loop {
        seen.extend(page);
        let Some(before) = next.as_str().map(str::to_owned) else {
            break;
        };
        (page, next) = ids(&app, &[("q", "平局"), ("limit", "1"), ("before", &before)]).await;
    }
    assert_eq!(seen, ["t3", "t2", "t1"]);
    assert_eq!(ids(&app, &[("q", "平局"), ("limit", "2")]).await.1, "t2");
}

#[tokio::test]
async fn folds_case_for_ascii_letters_only() {
    let app = fixture().await;
    sqlx::query(
        "INSERT INTO bot_messages(bot_id,id,kind,content) VALUES ('a','cafe','bot_reply','CAFÉ')",
    )
    .execute(&app.db)
    .await
    .unwrap();
    assert_eq!(ids(&app, &[("q", "cafÉ")]).await.0, ["cafe"]);
    assert!(ids(&app, &[("q", "café")]).await.0.is_empty());
}

#[tokio::test]
async fn rejects_invalid_requests() {
    let app = fixture().await;
    for query in [
        vec![("q", "  ")],
        vec![("q", "部署"), ("limit", "0")],
        vec![("q", "部署"), ("limit", "101")],
        vec![("q", "部署"), ("before", "other")],
    ] {
        let (status, body) = search(&app, "a", &query).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{query:?}: {body}");
    }
    let long = "字".repeat(201);
    assert_eq!(
        search(&app, "a", &[("q", &long)]).await.0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(search(&app, "a", &[]).await.0, StatusCode::BAD_REQUEST);
    assert_eq!(
        search(&app, "missing", &[("q", "部署")]).await.0,
        StatusCode::NOT_FOUND
    );
}

async fn chat_tool(
    app: &TestApp,
    tool: &str,
    session: &str,
    credential: &str,
    input: Value,
) -> (StatusCode, Value) {
    use super::support::http::{TOKEN, json_response};
    use axum::{
        body::Body,
        http::{Request, header},
    };
    use tower::ServiceExt;

    let response = pontia_http::router(app.state.clone())
        .oneshot(
            Request::post(format!("/api/v1/bot/{tool}"))
                .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
                .header(header::CONTENT_TYPE, "application/json")
                .header(pontia_bot::identity::TOOL_TOKEN_HEADER, credential)
                .body(Body::from(
                    json!({"session_id":session,"input":input}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    json_response(response).await
}

async fn retrieval_fixture() -> TestApp {
    let app = fixture().await;
    sqlx::query(r#"INSERT INTO sessions(session_id,client_type,state,role,credential_env, credential_sha256)
        VALUES ('reader','generic','idle','retrieval','PONTIA_BOT_TOOL_TOKEN', x'dd722eebece24666ffa703d6678b8b0562b384f78c6cf4c19372e4bef7d197d2'),
               ('other-reader','generic','idle','retrieval','PONTIA_BOT_TOOL_TOKEN', x'fa4b5f8f6c17e2865852fbaee0ae55fbed9f9e2a9a6d51e88591f790a3796654'),
               ('helper','generic','idle',NULL,'PONTIA_BOT_TOOL_TOKEN', x'f9e34ac7187d217f096f93dcd488d76004f4872c60a0b0a586c4561018249e04');
        INSERT INTO bot_helpers(reference,bot_id,session_id) VALUES
            ('helper:a:r','a','reader'), ('helper:b:r','b','other-reader'), ('helper:a:h','a','helper');"#)
        .execute(&app.db).await.unwrap();
    app
}

#[tokio::test]
async fn retrieval_search_and_chronological_context_stay_in_the_requesting_bot_chat() {
    let app = retrieval_fixture().await;
    for (session, credential, input, expected) in [
        (
            "reader",
            "cred-a",
            json!({"action":"search","keyword":"部署","limit":10}),
            vec!["m2", "m1"],
        ),
        (
            "other-reader",
            "cred-b",
            json!({"action":"search","keyword":"部署","limit":10}),
            vec!["other"],
        ),
        (
            "reader",
            "cred-a",
            json!({"action":"context","message_id":"t1","radius":2}),
            vec!["m6", "m7", "t1", "t2", "t3"],
        ),
        (
            "reader",
            "cred-a",
            json!({"action":"context","message_id":"t1","radius":0}),
            vec!["t1"],
        ),
    ] {
        let (status, body) = chat_tool(&app, "chat-history", session, credential, input).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let ids: Vec<_> = body["data"]["messages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|message| message["id"].as_str().unwrap())
            .collect();
        assert_eq!(ids, expected);
    }
    for (input, status) in [
        (
            json!({"action":"context","message_id":"other","radius":1}),
            StatusCode::NOT_FOUND,
        ),
        (
            json!({"action":"search","keyword":"部署","limit":1,"before":"other"}),
            StatusCode::BAD_REQUEST,
        ),
        (
            json!({"action":"search","keyword":"部署","limit":1,"bot_id":"b"}),
            StatusCode::BAD_REQUEST,
        ),
        (
            json!({"action":"context","message_id":"t1","radius":51}),
            StatusCode::BAD_REQUEST,
        ),
    ] {
        assert_eq!(
            chat_tool(&app, "chat-history", "reader", "cred-a", input)
                .await
                .0,
            status
        );
    }
}

#[tokio::test]
async fn chat_reads_require_a_live_retrieval_credential_and_grant_no_write_permission() {
    let app = retrieval_fixture().await;
    let search = json!({"action":"search","keyword":"部署","limit":1});
    for (session, credential) in [("other-reader", "cred-a"), ("helper", "cred-h")] {
        assert_eq!(
            chat_tool(&app, "chat-history", session, credential, search.clone())
                .await
                .0,
            StatusCode::FORBIDDEN
        );
    }
    assert_eq!(
        chat_tool(
            &app,
            "send-to-user",
            "reader",
            "cred-a",
            json!({"message":"unauthorized"})
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert!(ids(&app, &[("q", "unauthorized")]).await.0.is_empty());
    sqlx::query("UPDATE sessions SET state='exited' WHERE session_id='reader'")
        .execute(&app.db)
        .await
        .unwrap();
    assert_eq!(
        chat_tool(&app, "chat-history", "reader", "cred-a", search)
            .await
            .0,
        StatusCode::FORBIDDEN
    );
}
