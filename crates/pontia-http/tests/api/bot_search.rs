use super::{common::test_app::TestApp, support::http::get_json};
use axum::http::StatusCode;
use serde_json::Value;

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
