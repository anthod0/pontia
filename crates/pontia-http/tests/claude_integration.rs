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

async fn wait_interactions(app: &AppState, session: &str, states: &[&str]) -> Vec<Value> {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let (status, body) = call(
                app,
                "GET",
                &format!("/api/v1/sessions/{session}/interactions"),
                Value::Null,
            )
            .await;
            assert_eq!(status, StatusCode::OK, "{body}");
            let mut items = body["data"]["interactions"].as_array().unwrap().clone();
            items.sort_by_key(|v| v["interaction_id"].as_str().unwrap().to_owned());
            if items
                .iter()
                .map(|v| v["state"].as_str().unwrap())
                .collect::<Vec<_>>()
                == states
            {
                return items;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap()
}

struct Harness {
    app: AppState,
    root: tempfile::TempDir,
    tasks: pontia_application::clients::ClientServiceTasks,
}
impl Harness {
    async fn new() -> Self {
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
        Self { app, root, tasks }
    }
    async fn approvals(&self) -> (String, String, Vec<String>) {
        let (status, created) = call(&self.app, "POST", "/api/v1/sessions", json!({"client_type":"claude-agent","workspace":self.root.path(),"initial_task":{"input":"approval-pair"}})).await;
        assert_eq!(status, StatusCode::CREATED);
        let session = created["data"]["session"]["session_id"]
            .as_str()
            .unwrap()
            .to_owned();
        self.gate(&session, "start");
        let turn = wait_turn(&self.app, &session, "running").await;
        let items = wait_interactions(&self.app, &session, &["pending", "pending"]).await;
        let paths = items
            .iter()
            .map(|v| {
                format!(
                    "/api/v1/sessions/{session}/interactions/{}/response",
                    v["interaction_id"].as_str().unwrap()
                )
            })
            .collect();
        (session, turn, paths)
    }
    fn gate(&self, session: &str, action: &str) {
        std::fs::write(self.root.path().join(format!("{action}-{session}-1")), "").unwrap();
    }
    fn choices(&self, session: &str) -> Vec<Value> {
        std::fs::read_to_string(self.root.path().join("wire.jsonl"))
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .filter(|v| v["session"] == session && v["method"] == "permission")
            .collect()
    }
    async fn stop(self) {
        self.app.shutdown().notify();
        tokio::time::timeout(Duration::from_secs(5), self.tasks.join())
            .await
            .unwrap()
            .unwrap();
    }
}

#[tokio::test]
async fn claude_http_invalid_choice_preserves_pending_callback() {
    let h = Harness::new().await;
    let (session, _, paths) = h.approvals().await;
    assert_eq!(
        call(&h.app, "PUT", &paths[0], json!({"option_key":"always"}))
            .await
            .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    wait_interactions(&h.app, &session, &["pending", "pending"]).await;
    assert!(h.choices(&session).is_empty());
    h.stop().await;
}

#[tokio::test]
async fn claude_http_competing_and_duplicate_choices_deliver_only_once() {
    let h = Harness::new().await;
    let (session, _, paths) = h.approvals().await;
    let (a, b) = tokio::join!(
        call(&h.app, "PUT", &paths[0], json!({"option_key":"allow"})),
        call(&h.app, "PUT", &paths[0], json!({"option_key":"deny"}))
    );
    assert!(a.0 == StatusCode::OK || b.0 == StatusCode::OK);
    assert!([StatusCode::OK, StatusCode::CONFLICT].contains(&a.0));
    assert!([StatusCode::OK, StatusCode::CONFLICT].contains(&b.0));
    let items = wait_interactions(&h.app, &session, &["resolved", "pending"]).await;
    let (status, retried) = call(&h.app, "PUT", &paths[0], json!({"option_key":"deny"})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        retried["data"]["interaction"]["response"],
        items[0]["response"]
    );
    assert_eq!(h.choices(&session).len(), 1);
    h.stop().await;
}

#[tokio::test]
async fn claude_http_parallel_approvals_deliver_allow_and_deny_to_separate_callbacks() {
    let h = Harness::new().await;
    let (session, _, paths) = h.approvals().await;
    for (path, option) in paths.iter().zip(["allow", "deny"]) {
        assert_eq!(
            call(&h.app, "PUT", path, json!({"option_key":option}))
                .await
                .0,
            StatusCode::OK
        );
    }
    wait_interactions(&h.app, &session, &["resolved", "resolved"]).await;
    let choices = h.choices(&session);
    assert_eq!(choices.len(), 2);
    assert_eq!(choices[0]["option"], "allow");
    assert_eq!(choices[1]["option"], "deny");
    assert_ne!(choices[0]["permission_id"], choices[1]["permission_id"]);
    wait_turn(&h.app, &session, "running").await;
    h.stop().await;
}

async fn assert_expired(h: &Harness, session: &str, paths: &[String]) {
    wait_interactions(&h.app, session, &["expired", "expired"]).await;
    for path in paths {
        assert_eq!(
            call(&h.app, "PUT", path, json!({"option_key":"allow"}))
                .await
                .0,
            StatusCode::CONFLICT
        );
    }
    assert!(h.choices(session).is_empty());
    wait_turn(&h.app, session, "running").await;
}

#[tokio::test]
async fn claude_http_native_cancellation_rejects_late_choices() {
    let h = Harness::new().await;
    let (session, _, paths) = h.approvals().await;
    h.gate(&session, "cancel");
    assert_expired(&h, &session, &paths).await;
    h.stop().await;
}

#[tokio::test]
async fn claude_http_host_loss_expires_callbacks_without_turn_result() {
    let h = Harness::new().await;
    let (session, _, paths) = h.approvals().await;
    h.gate(&session, "lost");
    assert_expired(&h, &session, &paths).await;
    h.stop().await;
}

#[tokio::test]
async fn claude_http_interrupt_remains_available_while_approvals_wait() {
    let h = Harness::new().await;
    let (session, turn, paths) = h.approvals().await;
    assert_eq!(
        call(
            &h.app,
            "POST",
            &format!("/api/v1/sessions/{session}/turns/{turn}/interrupt"),
            json!({})
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_expired(&h, &session, &paths).await;
    h.stop().await;
}

#[tokio::test]
async fn claude_http_creation_inbox_live_output_interrupt_and_exit_share_application_facts() {
    let Harness { app, root, tasks } = Harness::new().await;
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
