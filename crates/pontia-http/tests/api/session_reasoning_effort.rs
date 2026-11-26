use crate::common::{generic_client::GenericClientTestScope, test_app::TestApp};
use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use pontia_agent_control::{
    ClientControlChannel, ClientControlOperation,
    client_contract::{
        AgentClientCapabilities, AgentClientSpec, DispatchBehavior, TEST_SPEC, test_registration,
    },
    clients::ClientRegistry,
};
use pontia_application::AppState;
use serde_json::{Value, json};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use tower::ServiceExt;

const TOKEN: &str = "test-token";
const SPEC: AgentClientSpec = AgentClientSpec {
    client_type: "reasoning-effort-test",
    capabilities: AgentClientCapabilities {
        reasoning_effort_control: true,
        ..TEST_SPEC.capabilities
    },
    adapter: pontia_agent_control::client_contract::AgentClientAdapter {
        dispatch: DispatchBehavior::Connected,
        ..TEST_SPEC.adapter
    },
    ..TEST_SPEC
};

#[derive(Default)]
struct Channel {
    efforts: Mutex<Vec<String>>,
    closed: AtomicBool,
}

impl ClientControlChannel for Channel {
    fn set_reasoning_effort<'a>(&'a self, effort: &'a str) -> ClientControlOperation<'a> {
        Box::pin(async move {
            self.efforts.lock().unwrap().push(effort.into());
            Ok(())
        })
    }
    fn available(&self) -> bool {
        !self.closed.load(Ordering::SeqCst)
    }
    fn invalidate(&self) {
        self.closed.store(true, Ordering::SeqCst);
    }
    fn interrupt(&self) -> ClientControlOperation<'_> {
        Box::pin(async { Ok(()) })
    }
    fn shutdown(&self) -> ClientControlOperation<'_> {
        Box::pin(async { Ok(()) })
    }
    fn ping(&self) -> ClientControlOperation<'_> {
        Box::pin(async { Ok(()) })
    }
    fn replay<'a>(&'a self, _: &'a str) -> ClientControlOperation<'a> {
        Box::pin(async { Ok(()) })
    }
    fn submit<'a>(&'a self, _: &'a str, _: Option<&'a str>) -> ClientControlOperation<'a> {
        Box::pin(async { Ok(()) })
    }
}

async fn patch(state: AppState, session: &str, effort: &str) -> (StatusCode, Value) {
    let response = pontia_http::router(state)
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/api/v1/sessions/{session}/reasoning-effort"))
                .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"reasoning_effort":effort}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&body).unwrap())
}

#[tokio::test]
async fn supported_control_returns_accepted_and_dispatches_without_forging_state() {
    let root = tempfile::tempdir().unwrap();
    let db = pontia_storage_sqlite::connect_sqlite("sqlite::memory:")
        .await
        .unwrap();
    pontia_storage_sqlite::run_migrations(&db).await.unwrap();
    let mut registration = test_registration();
    registration.spec = &SPEC;
    registration.in_process = None;
    let mut clients = ClientRegistry::default();
    clients.register(registration);
    let state = AppState::builder(
        pontia_agent_control::AgentControl::builder(db.clone(), root.path().into())
            .clients(clients)
            .build(),
    )
    .external_api_token(Some(TOKEN.into()))
    .build();
    sqlx::query("INSERT INTO sessions(session_id,client_type,state) VALUES ('session','reasoning-effort-test','busy')")
        .execute(&db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO session_runtimes(session_id,runtime_id,role,state,created_at) VALUES ('session','runtime','generic_control','running','2026-01-01T00:00:00Z')")
        .execute(&db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO agent_bindings(id,session_id,client_type,launch_cwd,client_session_key,metadata) VALUES ('binding','session','reasoning-effort-test','/unused','native','{}')")
        .execute(&db)
        .await
        .unwrap();
    let channel = Arc::new(Channel::default());
    state
        .control()
        .client_control()
        .attach(
            SPEC.client_type,
            "session",
            "runtime",
            "native",
            channel.clone(),
        )
        .await
        .unwrap();

    let (status, body) = patch(state, "session", "high").await;

    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(body["data"], json!({"accepted":true}));
    assert_eq!(channel.efforts.lock().unwrap().as_slice(), ["high"]);
    let effort: Option<String> = sqlx::query_scalar(
        "SELECT json_extract(metadata,'$.reasoning_effort') FROM sessions WHERE session_id='session'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(effort, None);
}

#[tokio::test]
async fn control_validates_input_and_session_capability() {
    let _scope = GenericClientTestScope::new().await;
    let (state, _pontia_home) = TestApp::builder()
        .external_api_token(Some(TOKEN.into()))
        .build_state()
        .await;
    sqlx::query(
        "INSERT INTO sessions(session_id,client_type,state) VALUES ('session','generic','idle')",
    )
    .execute(&state.db())
    .await
    .unwrap();

    let (status, body) = patch(state.clone(), "session", "  ").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "invalid_request");

    let (status, body) = patch(state, "session", "high").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"]["code"], "capability_unavailable");
}
