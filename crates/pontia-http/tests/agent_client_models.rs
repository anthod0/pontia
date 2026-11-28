mod common;
use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use common::test_app::EnvGuard;
use pontia_agent_control::{agent_clients::AgentClientService, clients::ClientRegistry};
use pontia_application::AppState;
use pontia_storage_sqlite::{connect_sqlite, run_migrations};
use serde_json::{Value, json};
use std::os::unix::fs::PermissionsExt;
use tower::ServiceExt;

fn clients_with_pi() -> ClientRegistry {
    let mut clients = ClientRegistry::default();
    clients.register(pontia_client_pi::registration());
    clients.register(pontia_client_codex::registration());
    clients
}

async fn fixture_with_state(
    response: Value,
    state: Value,
) -> (Router, sqlx::SqlitePool, tempfile::TempDir, EnvGuard) {
    let root = tempfile::tempdir().unwrap();
    let mut env = EnvGuard::new();
    env.set(
        "PATH",
        std::env::join_paths(
            std::iter::once(root.path().to_owned())
                .chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
        )
        .unwrap(),
    );
    let script = root.path().join("pi");
    std::fs::write(
        &script,
        format!(
            r#"#!/usr/bin/env python3
import json, os, sys
from pathlib import Path
if sys.argv[1:] == ['--version']:
    print('1.2.3')
    sys.exit(0)
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
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
    let clients = clients_with_pi();
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
    (crate::common::http::router(state), db, root, env)
}

async fn fixture(response: Value) -> (Router, sqlx::SqlitePool, tempfile::TempDir, EnvGuard) {
    fixture_with_state(response, json!({"success":true,"data":{}})).await
}

async fn request(app: &Router, client: &str, authorized: bool) -> (StatusCode, Value) {
    models_request(app, client, authorized, "").await
}

async fn models_request(
    app: &Router,
    client: &str,
    authorized: bool,
    query: &str,
) -> (StatusCode, Value) {
    let mut request =
        Request::builder().uri(format!("/api/v1/agent-clients/{client}/models{query}"));
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
    let (app, db, root, _env) = fixture_with_state(
        json!({"success":true,"data":{"models":[
            {"provider":"one","id":"same","name":"First"},
            {"provider":"two","id":"same","name":"Second","reasoning":true,"thinkingLevelMap":{"off":null,"minimal":null,"low":null,"max":"max"}}
        ]}}),
        json!({"success":true,"data":{"thinkingLevel":"medium","model":{"provider":"two","id":"same","name":"Second"}}}),
    )
    .await;
    let (status, response) = request(&app, "pi/default", true).await;
    assert_eq!(status, StatusCode::OK, "{response}");
    assert_eq!(response["data"]["default_model"], "two/same");
    assert_eq!(
        response["data"]["models"],
        json!([
            {"id":"one/same","name":"First","description":"one","reasoning_efforts":[],"default_reasoning_effort":null},
            {"id":"two/same","name":"Second","description":"two","default_reasoning_effort":"medium","reasoning_efforts":[
                {"id":"medium","name":"medium","description":""},
                {"id":"high","name":"high","description":""},
                {"id":"max","name":"max","description":""}
            ]}
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
async fn startup_probe_warms_the_catalog_used_by_cached_http_queries() {
    let (app, db, root, _env) = fixture(json!({"success":true,"data":{"models":[
        {"provider":"one","id":"first","name":"First"}
    ]}}))
    .await;
    let script = root.path().join("pi");
    let service = AgentClientService::new(db, clients_with_pi());
    service
        .clone()
        .probe_enabled(root.path().into(), false)
        .await;
    assert!(!root.path().join("pid").exists());

    service.save("pi", "default", false).await.unwrap();
    service
        .clone()
        .probe_enabled(root.path().into(), true)
        .await;
    assert!(!root.path().join("pid").exists());

    service.save("pi", "default", true).await.unwrap();
    service
        .clone()
        .probe_enabled(root.path().into(), true)
        .await;
    std::fs::remove_file(root.path().join("pid")).unwrap();
    service.probe_enabled(root.path().into(), true).await;
    assert!(!root.path().join("pid").exists());
    std::fs::remove_file(script).unwrap();
    let (status, cached) = models_request(&app, "pi/default", true, "?prefer_cache=true").await;
    assert_eq!(status, StatusCode::OK, "{cached}");
    assert_eq!(cached["data"]["models"][0]["id"], "one/first");
    assert_eq!(cached["data"]["version"], "1.2.3");
}

#[tokio::test]
async fn cached_catalog_survives_version_change_and_failed_refresh() {
    let (app, db, root, _env) = fixture(json!({"success":true,"data":{"models":[
        {"provider":"one","id":"first","name":"First","reasoning":true}
    ]}}))
    .await;
    sqlx::query("UPDATE agent_clients SET version = 'v1'")
        .execute(&db)
        .await
        .unwrap();
    let query = "?prefer_cache=true";
    let (status, first) = models_request(&app, "pi/default", true, query).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(first["data"]["version"], "v1");
    assert!(first["data"]["fetched_at"].is_i64());
    let script = root.path().join("pi");
    let original = std::fs::read_to_string(&script).unwrap();
    std::fs::write(&script, "raise RuntimeError('offline')").unwrap();
    sqlx::query("UPDATE agent_clients SET version = 'v2'")
        .execute(&db)
        .await
        .unwrap();
    assert!(!request(&app, "pi/default", true).await.0.is_success());
    let (status, cached) = models_request(&app, "pi/default", true, query).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(cached["data"], first["data"]);
    std::fs::write(&script, original.replace("First", "Second")).unwrap();
    let (status, latest) = request(&app, "pi/default", true).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(latest["data"]["version"], "v2");
    assert_eq!(latest["data"]["models"][0]["name"], "Second");
    assert_eq!(
        models_request(&app, "pi/default", true, query).await.1["data"],
        latest["data"]
    );
    sqlx::query("UPDATE agent_clients SET enabled = 0")
        .execute(&db)
        .await
        .unwrap();
    assert!(
        !models_request(&app, "pi/default", true, query)
            .await
            .0
            .is_success()
    );
}

#[tokio::test]
async fn non_reasoning_default_model_reports_its_native_default_without_selectable_efforts() {
    let model = json!({"provider":"provider","id":"chat","name":"Chat","reasoning":false});
    let (app, _, _root, _env) = fixture_with_state(
        json!({"success":true,"data":{"models":[model]}}),
        json!({"success":true,"data":{"model":model,"thinkingLevel":"off"}}),
    )
    .await;
    let (status, response) = request(&app, "pi/default", true).await;
    assert_eq!(status, StatusCode::OK);
    let model = &response["data"]["models"][0];
    assert_eq!(model["reasoning_efforts"], json!([]));
    assert_eq!(model["default_reasoning_effort"], "off");
}

#[tokio::test]
async fn requires_authentication_and_an_enabled_client_before_starting_a_process() {
    let (app, db, root, _env) = fixture(json!({"success":true,"data":{"models":[]}})).await;
    assert_eq!(
        request(&app, "pi/default", false).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert!(!root.path().join("pid").exists());
    sqlx::query("UPDATE agent_clients SET enabled = 0")
        .execute(&db)
        .await
        .unwrap();
    let (status, response) = request(&app, "pi/default", true).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(response["error"]["code"], "client_disabled");
    assert!(!root.path().join("pid").exists());
}

#[tokio::test]
async fn returns_empty_catalog_and_propagates_native_failures() {
    let (app, _, _root, _env) = fixture(json!({"success":true,"data":{"models":[]}})).await;
    let (status, response) = request(&app, "pi/default", true).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(response["data"]["models"], json!([]));
    assert_eq!(response["data"].get("default_model"), Some(&Value::Null));
    drop(_env);
    for response in [
        json!({"success":false,"error":"Cannot load models"}),
        json!({"success":true,"data":{"models":[{"provider":"one","id":"","name":"Invalid"}]}}),
    ] {
        let (app, _, _root, _env) = fixture(response).await;
        assert!(!request(&app, "pi/default", true).await.0.is_success());
    }
}

#[tokio::test]
async fn default_model_lookup_failure_is_not_reported_as_no_default() {
    let (app, _, _root, _env) = fixture_with_state(
        json!({"success":true,"data":{"models":[]}}),
        json!({"success":false,"error":"Cannot read state"}),
    )
    .await;
    let (status, response) = request(&app, "pi/default", true).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(response["error"]["code"], "unavailable");
}

#[tokio::test]
async fn times_out_a_stalled_catalog_and_stops_its_process() {
    let (app, _, root, _env) = fixture(json!({"hang": true})).await;
    let (status, response) = request(&app, "pi/default", true).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(response["error"]["code"], "unavailable");
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

#[path = "agent_client_models/bot_profiles.rs"]
mod bot_profiles;
