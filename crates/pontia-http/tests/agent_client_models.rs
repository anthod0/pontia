use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use pontia_agent_control::{agent_clients::AgentClientService, clients::ClientRegistry};
use pontia_application::AppState;
use pontia_storage_sqlite::{connect_sqlite, run_migrations};
use serde_json::{Value, json};
use std::sync::Arc;
use tower::ServiceExt;

async fn fixture_with_state(
    response: Value,
    state: Value,
) -> (Router, sqlx::SqlitePool, tempfile::TempDir) {
    let root = tempfile::tempdir().unwrap();
    let script = root.path().join("catalog.py");
    std::fs::write(
        &script,
        format!(
            r#"
import json, os, sys
from pathlib import Path
assert sys.argv[1:] == ['--mode', 'rpc', '--no-session']
assert Path.cwd() == Path(__file__).parent
request = json.loads(sys.stdin.readline())
assert request['type'] == 'get_available_models'
Path('pid').write_text(str(os.getpid()))
print(json.dumps({{'type': 'event', 'message': 'ready'}}), flush=True)
response = json.loads({response:?})
if response.get('hang'):
    sys.stdin.read()
    sys.exit(0)
response.update(type='response', id=request['id'])
print(json.dumps(response), flush=True)
if response.get('success'):
    request = json.loads(sys.stdin.readline())
    assert request['type'] == 'get_state'
    state = json.loads({state:?})
    state.update(type='response', id=request['id'])
    print(json.dumps(state), flush=True)
assert sys.stdin.read() == ''
Path('closed').touch()
"#,
            response = response.to_string(),
            state = state.to_string()
        ),
    )
    .unwrap();
    let db = connect_sqlite("sqlite::memory:").await.unwrap();
    run_migrations(&db).await.unwrap();
    let mut clients = ClientRegistry::default();
    let mut pi = pontia_client_pi::registration(None);
    pi.configuration = Some(Arc::new(pontia_client_pi::config::PiConfiguration::new(
        json!({"tui_command":format!("python3 '{}'", script.display())}),
        json!({}),
    )));
    clients.register(pi);
    clients.register(pontia_client_codex::registration());
    AgentClientService::new(db.clone(), clients.clone())
        .save("pi", "default", true)
        .await
        .unwrap();
    let state = AppState::builder(
        pontia_agent_control::AgentControl::builder(db.clone(), root.path().into())
            .clients(clients)
            .build(),
    )
    .external_api_token(Some("test-token".into()))
    .build();
    (pontia_http::router(state), db, root)
}

async fn fixture(response: Value) -> (Router, sqlx::SqlitePool, tempfile::TempDir) {
    fixture_with_state(response, json!({"success":true,"data":{}})).await
}

async fn request(app: &Router, client: &str, authorized: bool) -> (StatusCode, Value) {
    let mut request = Request::builder().uri(format!("/api/v1/agent-clients/{client}/models"));
    if authorized {
        request = request.header("Authorization", "Bearer test-token");
    }
    let response = app
        .clone()
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&body).unwrap())
}

#[tokio::test]
async fn lists_configured_client_models_without_a_session_and_closes_the_process() {
    let (app, db, root) = fixture_with_state(
        json!({"success":true,"data":{"models":[
            {"provider":"one","id":"same","name":"First"},
            {"provider":"two","id":"same","name":"Second"}
        ]}}),
        json!({"success":true,"data":{"model":{"provider":"two","id":"same","name":"Second"}}}),
    )
    .await;
    let (status, response) = request(&app, "pi/default", true).await;
    assert_eq!(status, StatusCode::OK, "{response}");
    assert_eq!(response["data"]["default_model"], "two/same");
    assert_eq!(
        response["data"]["models"],
        json!([
            {"id":"one/same","name":"First","description":"one"},
            {"id":"two/same","name":"Second","description":"two"}
        ])
    );
    assert!(root.path().join("closed").exists());
    let sessions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(sessions, 0);
}

#[tokio::test]
async fn requires_authentication_and_an_enabled_client_before_starting_a_process() {
    let (app, db, root) = fixture(json!({"success":true,"data":{"models":[]}})).await;
    assert_eq!(
        request(&app, "pi/default", false).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert!(!root.path().join("pid").exists());
    sqlx::query("UPDATE agent_clients SET enabled = 0")
        .execute(&db)
        .await
        .unwrap();
    assert!(!request(&app, "pi/default", true).await.0.is_success());
    assert!(!root.path().join("pid").exists());
}

#[tokio::test]
async fn returns_empty_catalog_and_propagates_native_failures() {
    let (app, _, _root) = fixture(json!({"success":true,"data":{"models":[]}})).await;
    let (status, response) = request(&app, "pi/default", true).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(response["data"]["models"], json!([]));
    assert_eq!(response["data"].get("default_model"), Some(&Value::Null));
    for response in [
        json!({"success":false,"error":"Cannot load models"}),
        json!({"success":true,"data":{"models":[{"provider":"one","id":"","name":"Invalid"}]}}),
    ] {
        let (app, _, _root) = fixture(response).await;
        assert!(!request(&app, "pi/default", true).await.0.is_success());
    }
}

#[tokio::test]
async fn default_model_lookup_failure_is_not_reported_as_no_default() {
    let (app, _, _root) = fixture_with_state(
        json!({"success":true,"data":{"models":[]}}),
        json!({"success":false,"error":"Cannot read state"}),
    )
    .await;
    let (status, response) = request(&app, "pi/default", true).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(response["error"]["code"], "capability_unavailable");
}

#[tokio::test]
async fn times_out_a_stalled_catalog_and_stops_its_process() {
    let (app, _, root) = fixture(json!({"hang": true})).await;
    let (status, response) = request(&app, "pi/default", true).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(response["error"]["code"], "capability_unavailable");
    let pid = std::fs::read_to_string(root.path().join("pid")).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let running = std::process::Command::new("kill")
                .args(["-0", &pid])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .unwrap()
                .success();
            if !running {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}
