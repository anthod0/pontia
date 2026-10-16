use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use pontia_application::{AppState, client_contract::ClientServicePhase, clients::ClientRegistry};
use pontia_client_acp::registration;
use pontia_config::{AcpAgentConfig, AcpConfig};
use pontia_storage_sqlite::{connect_sqlite, run_migrations};
use serde_json::{Value, json};
use std::{collections::BTreeMap, time::Duration};
use tower::ServiceExt;

async fn call(app: &AppState, method: &str, path: &str, body: Value) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(path)
        .header("Authorization", "Bearer integration-token")
        .header("Content-Type", "application/json")
        .body(if body.is_null() {
            Body::empty()
        } else {
            Body::from(body.to_string())
        })
        .unwrap();
    let response = pontia_http::router(app.clone())
        .oneshot(request)
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes)
            .unwrap_or_else(|error| panic!("{method} {path}: {status}: {error}")),
    )
}

async fn get(app: &AppState, path: &str) -> Value {
    let (status, body) = call(app, "GET", path, Value::Null).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body["data"].clone()
}

async fn wait_turn(app: &AppState, session: &str, turn: &str, expected: &str) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let body = get(app, &format!("/api/v1/sessions/{session}/turns/{turn}")).await;
            if body["turn"]["state"] == expected {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn configured_agent_selection_flows_through_http_creation_input_permission_cancel_history_and_exit()
 {
    let root = tempfile::tempdir().unwrap();
    let config = AcpAgentConfig {
        name: Some("Company Agent".into()),
        registry_id: None,
        command: "python3".into(),
        args: vec![format!(
            "{}/../pontia-client-acp/tests/fixtures/agent.py",
            env!("CARGO_MANIFEST_DIR")
        )],
        env: BTreeMap::from([
            ("ACP_FIXTURE_ROOT".into(), root.path().display().to_string()),
            ("ACP_PERMISSION".into(), "1".into()),
            ("PRIVATE_SECRET".into(), "never-expose".into()),
        ]),
    };
    let mut clients = ClientRegistry::default();
    clients.register(registration(AcpConfig {
        agents: BTreeMap::from([("company-id".into(), config)]),
    }));
    let pool = connect_sqlite("sqlite::memory:").await.unwrap();
    run_migrations(&pool).await.unwrap();
    let app = AppState::builder(pool.clone(), root.path().into())
        .clients(clients.clone())
        .external_api_token(Some("integration-token".into()))
        .build();
    let tasks = clients
        .start_services(app.clone(), ClientServicePhase::Transport)
        .await
        .unwrap();
    let options = get(&app, "/api/v1/session-creation-options").await;
    assert_eq!(options["options"].as_array().unwrap().len(), 1);
    let option = &options["options"][0];
    assert_eq!(option["client_type"], "acp");
    assert_eq!(option["metadata"], json!({"acp_agent_id":"company-id"}));
    assert!(!options.to_string().contains("never-expose"));
    let (status, result) = call(&app, "POST", "/api/v1/sessions", json!({
        "client_type":option["client_type"], "metadata":option["metadata"], "workspace":root.path(),
    })).await;
    assert_eq!(status, StatusCode::CREATED, "{result}");
    let session = result["data"]["session"]["session_id"].as_str().unwrap();
    let path = format!("/api/v1/sessions/{session}");
    let detail = get(&app, &path).await;
    assert_eq!(detail["session"]["acp"]["agent_id"], "company-id");
    assert_eq!(detail["session"]["acp"]["agent_info"]["name"], "fixture");
    assert_eq!(detail["session"]["capabilities"]["timeline"], true);
    assert_eq!(detail["session"]["capabilities"]["topology"], false);
    let (status, result) = call(
        &app,
        "POST",
        &format!("{path}/inbox/messages"),
        json!({"input":"end_turn","delivery_policy":"after_idle"}),
    )
    .await;
    assert!(status.is_success(), "{result}");
    let turn = result["data"]["inbox_message"]["turn_id"].as_str().unwrap();
    std::fs::write(root.path().join("update-1"), "").unwrap();
    wait_turn(&app, session, turn, "running").await;
    let interactions = get(&app, &format!("{path}/interactions")).await;
    let interactions = interactions["interactions"].as_array().unwrap();
    assert_eq!(interactions.len(), 3);
    let id = interactions[0]["interaction_id"].as_str().unwrap();
    let (status, body) = call(
        &app,
        "PUT",
        &format!("{path}/interactions/{id}/response"),
        json!({"option_key":"always:原"}),
    )
    .await;
    assert!(status.is_success(), "{body}");
    let (status, body) = call(&app, "POST", &format!("{path}/interrupt"), json!({})).await;
    assert!(status.is_success(), "{body}");
    let turn_before_response = get(&app, &format!("{path}/turns/{turn}")).await;
    assert_eq!(turn_before_response["turn"]["state"], "running");
    std::fs::write(root.path().join("release-1"), "").unwrap();
    wait_turn(&app, session, turn, "interrupted").await;
    let history = get(&app, &format!("{path}/turns/timeline?direction=backward")).await;
    assert!(history.to_string().contains("streamed answer"), "{history}");
    let (status, body) = call(&app, "POST", &format!("{path}/exit"), json!({})).await;
    assert!(status.is_success(), "{body}");
    let detail = get(&app, &path).await;
    assert_eq!(detail["session"]["state"], "exited");
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM session_runtimes WHERE session_id=?")
            .bind(session)
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
    app.shutdown().notify();
    tokio::time::timeout(Duration::from_secs(5), tasks.join())
        .await
        .unwrap()
        .unwrap();
}
