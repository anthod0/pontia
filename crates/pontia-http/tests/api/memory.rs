use crate::common::http;
use crate::common::test_app::TestApp;
use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use pontia_application::AppState;
use serde_json::{Value, json};
use tower::ServiceExt;

const TOKEN: &str = "test-token";

async fn get(state: AppState, uri: &str, token: Option<&str>) -> (StatusCode, Value) {
    let mut request = Request::builder().method("GET").uri(uri);
    if let Some(token) = token {
        request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    let response = http::router(state)
        .oneshot(request.body(Body::empty()).expect("request"))
        .await
        .expect("response");
    let status = response.status();
    let body = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    (status, serde_json::from_slice(&body).expect("json body"))
}

#[tokio::test]
async fn dashboard_reads_memories_and_user_file_with_the_external_api_token() {
    let (state, pontia_home) = TestApp::builder()
        .database_name("memory.db")
        .external_api_token(Some(TOKEN.to_string()))
        .build_state()
        .await;
    for (id, content, updated_at, archived) in [
        ("mem_a", "Dashboard uses bun", "2026-11-01T00:00:00Z", 0),
        ("mem_b", "Rust stable toolchain", "2026-11-02T00:00:00Z", 0),
        ("mem_c", "Dashboard archived", "2026-11-03T00:00:00Z", 1),
    ] {
        sqlx::query("INSERT INTO bot_memories (id, kind, content, keywords, archived, created_by, created_at, updated_at) VALUES (?, 'fact', ?, '[\"k\"]', ?, 'bot', ?, ?)")
            .bind(id).bind(content).bind(archived).bind(updated_at).bind(updated_at)
            .execute(&state.db()).await.unwrap();
    }

    for uri in ["/api/v1/memories", "/api/v1/memory/user"] {
        assert_eq!(
            get(state.clone(), uri, None).await.0,
            StatusCode::UNAUTHORIZED
        );
    }

    let (status, body) = get(state.clone(), "/api/v1/memories", Some(TOKEN)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let ids: Vec<_> = body["data"]["memories"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].clone())
        .collect();
    assert_eq!(ids, vec![json!("mem_b"), json!("mem_a")]);
    assert_eq!(body["data"]["total"], 2);
    assert!(body["data"]["next_cursor"].is_null());

    let (_, body) = get(
        state.clone(),
        "/api/v1/memories?q=dashboard&limit=1",
        Some(TOKEN),
    )
    .await;
    assert_eq!(body["data"]["memories"][0]["id"], "mem_a");
    assert_eq!(body["data"]["total"], 1);

    let (status, _) = get(state.clone(), "/api/v1/memories?cursor=bad", Some(TOKEN)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, body) = get(state.clone(), "/api/v1/memory/user", Some(TOKEN)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["data"]["user"],
        json!({"text": "", "character_limit": 2000, "updated_at": null})
    );

    std::fs::create_dir_all(pontia_home.path().join("bot")).unwrap();
    std::fs::write(pontia_home.path().join("bot/USER.md"), "# 关于我\n").unwrap();
    let (_, body) = get(state, "/api/v1/memory/user", Some(TOKEN)).await;
    assert_eq!(body["data"]["user"]["text"], "# 关于我\n");
    assert!(body["data"]["user"]["updated_at"].is_string());
}

async fn put_user(state: AppState, body: Value, token: Option<&str>) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method("PUT")
        .uri("/api/v1/memory/user")
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(token) = token {
        request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    let response = http::router(state)
        .oneshot(request.body(Body::from(body.to_string())).expect("request"))
        .await
        .expect("response");
    let status = response.status();
    let body = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    (status, serde_json::from_slice(&body).expect("json body"))
}

#[tokio::test]
async fn dashboard_replaces_user_file_only_when_it_saw_the_current_text() {
    let (state, pontia_home) = TestApp::builder()
        .database_name("memory.db")
        .external_api_token(Some(TOKEN.to_string()))
        .build_state()
        .await;
    let edit = json!({"text": "名字：林\n", "previous_text": ""});
    assert_eq!(
        put_user(state.clone(), edit.clone(), None).await.0,
        StatusCode::UNAUTHORIZED
    );
    let (status, body) = put_user(state.clone(), edit, Some(TOKEN)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["user"]["text"], "名字：林\n");
    assert!(body["data"]["user"]["updated_at"].is_string());
    let path = pontia_home.path().join("bot/USER.md");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "名字：林\n");

    let (status, body) = put_user(
        state.clone(),
        json!({"text": "名字：陈\n", "previous_text": "旧内容"}),
        Some(TOKEN),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "state_conflict");
    let (status, body) = put_user(
        state.clone(),
        json!({"text": "字".repeat(2_001), "previous_text": "名字：林\n"}),
        Some(TOKEN),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    let (status, _) = put_user(state, json!({"text": "x"}), Some(TOKEN)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "名字：林\n");
}
