use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use pontia_application::{AppState, client_contract::ClientServicePhase, clients::ClientRegistry};
use pontia_storage_sqlite::{connect_sqlite, run_migrations};
use serde_json::{Value, json};
use std::time::Duration;
use tower::ServiceExt;

async fn call(app: &AppState, method: &str, path: &str, body: Value) -> (StatusCode, Value) {
    let response = pontia_http::router(app.clone())
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header("Authorization", "Bearer fixture-token")
                .header("Content-Type", "application/json")
                .body(if body.is_null() {
                    Body::empty()
                } else {
                    Body::from(body.to_string())
                })
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}
async fn wait_turn(app: &AppState, session: &str, expected: &str) -> String {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let (_, body) = call(
                app,
                "GET",
                &format!("/api/v1/sessions/{session}/turns"),
                Value::Null,
            )
            .await;
            if let Some(turn) = body["data"]["turns"]
                .as_array()
                .and_then(|turns| turns.iter().find(|turn| turn["state"] == expected))
            {
                return turn["turn_id"].as_str().unwrap().into();
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn claude_http_creation_inbox_live_output_interrupt_and_exit_share_application_facts() {
    let root = tempfile::tempdir().unwrap();
    let pool = connect_sqlite("sqlite::memory:").await.unwrap();
    run_migrations(&pool).await.unwrap();
    let mut clients = ClientRegistry::default();
    clients.register(pontia_client_claude::registration_with_host(Some(
        format!(
            "{}/../pontia-client-claude/tests/fixtures/host.mjs",
            env!("CARGO_MANIFEST_DIR")
        )
        .into(),
    )));
    let app = AppState::builder(pool, root.path().into())
        .clients(clients.clone())
        .external_api_token(Some("fixture-token".into()))
        .build();
    pontia_application::agent_clients::AgentClientService::new(app.db(), clients.clone())
        .save("claude-agent", "default", Some(true), None)
        .await
        .unwrap();
    let tasks = clients
        .start_services(app.clone(), ClientServicePhase::Transport)
        .await
        .unwrap();
    let (status, created) = call(&app, "POST", "/api/v1/sessions", json!({"client_type":"claude-agent","workspace":root.path(),"initial_task":{"input":"tool"}})).await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let session = created["data"]["session"]["session_id"].as_str().unwrap();
    std::fs::write(root.path().join(format!("start-{session}-1")), "").unwrap();
    let turn = wait_turn(&app, session, "running").await;
    let (_, queued) = call(
        &app,
        "POST",
        &format!("/api/v1/sessions/{session}/inbox/messages"),
        json!({"input":"second","delivery_policy":"after_idle"}),
    )
    .await;
    assert_eq!(queued["data"]["inbox_message"]["state"], "pending");
    let response = pontia_http::router(app.clone())
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/sessions/{session}/live-output/stream"))
                .header("Authorization", "Bearer fixture-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let mut stream = response.into_body();
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut text = false;
        let mut failed_tool = false;
        while !text || !failed_tool {
            let frame = stream.frame().await.unwrap().unwrap();
            if let Ok(bytes) = frame.into_data() {
                for data in std::str::from_utf8(&bytes)
                    .unwrap()
                    .lines()
                    .filter_map(|line| line.strip_prefix("data: "))
                {
                    let event: Value = serde_json::from_str(data).unwrap();
                    if let Some(items) = event["items"].as_array() {
                        text |= items.iter().any(|item| item["text"] == "tool");
                        failed_tool |= items.iter().any(|item| item["result"]["error"] == "denied");
                    }
                    if let Some(updates) = event["updates"].as_array() {
                        text |= updates.iter().any(|update| update["delta"] == "tool");
                        failed_tool |= updates
                            .iter()
                            .any(|update| update["result"]["error"] == "denied");
                    }
                }
            }
        }
    })
    .await
    .unwrap();
    let (status, result) = call(
        &app,
        "POST",
        &format!("/api/v1/sessions/{session}/turns/{turn}/interrupt"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{result}");
    wait_turn(&app, session, "running").await;
    std::fs::write(root.path().join(format!("finish-{session}-1")), "").unwrap();
    wait_turn(&app, session, "interrupted").await;
    std::fs::write(root.path().join(format!("start-{session}-2")), "").unwrap();
    wait_turn(&app, session, "running").await;
    let (status, result) = call(
        &app,
        "POST",
        &format!("/api/v1/sessions/{session}/exit"),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{result}");
    assert_eq!(result["data"]["session"]["state"], "exited");
    wait_turn(&app, session, "abandoned").await;
    app.shutdown().notify();
    tokio::time::timeout(Duration::from_secs(5), tasks.join())
        .await
        .unwrap()
        .unwrap();
}
