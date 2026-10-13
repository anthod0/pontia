use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use pontia_application::{
    AppState, ClientControlChannel, ClientControlOperation,
    client_contract::{
        AgentClientCapabilities, AgentClientSpec, DispatchBehavior, RuntimeBindingBehavior,
        TEST_SPEC, test_registration,
    },
    clients::ClientRegistry,
};
use pontia_core::{
    Error,
    domain::{EventSource, EventType, ReportedEvent},
};
use serde_json::{Value, json};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use tower::ServiceExt;

pub const SPEC: AgentClientSpec = AgentClientSpec {
    client_type: "compaction-test",
    capabilities: AgentClientCapabilities {
        manual_compaction: true,
        compaction_events: true,
        compaction_history: true,
        timeline: true,
        ..TEST_SPEC.capabilities
    },
    adapter: pontia_application::client_contract::AgentClientAdapter {
        dispatch: DispatchBehavior::Connected,
        runtime_binding: RuntimeBindingBehavior::SharedBackend,
        ..TEST_SPEC.adapter
    },
    ..TEST_SPEC
};
pub const UNSUPPORTED: AgentClientSpec = AgentClientSpec {
    capabilities: AgentClientCapabilities {
        manual_compaction: false,
        compaction_events: false,
        compaction_history: false,
        timeline: false,
        ..SPEC.capabilities
    },
    ..SPEC
};
pub const RUNTIME_BOUND: AgentClientSpec = AgentClientSpec {
    adapter: pontia_application::client_contract::AgentClientAdapter {
        runtime_binding: RuntimeBindingBehavior::Named {
            runtime_kind: "test",
        },
        ..SPEC.adapter
    },
    ..SPEC
};

#[derive(Default)]
pub struct Channel {
    pub operations: Mutex<Vec<String>>,
    pub closed: AtomicBool,
    pub error: Mutex<Option<Error>>,
    pub delayed: AtomicBool,
    pub started: tokio::sync::Notify,
    pub finish: tokio::sync::Notify,
}
impl ClientControlChannel for Channel {
    fn compact<'a>(&'a self, operation_id: &'a str) -> ClientControlOperation<'a> {
        Box::pin(async move {
            self.operations.lock().unwrap().push(operation_id.into());
            if self.delayed.load(Ordering::SeqCst) {
                self.started.notify_one();
                self.finish.notified().await;
            }
            match self.error.lock().unwrap().take() {
                Some(error) => Err(error),
                None => Ok(()),
            }
        })
    }
    fn available(&self) -> bool {
        !self.closed.load(Ordering::SeqCst)
    }
    fn invalidate(&self) {
        self.closed.store(true, Ordering::SeqCst);
    }
    fn list_models(
        &self,
    ) -> ClientControlOperation<'_, Vec<pontia_application::sessions::SessionModel>> {
        Box::pin(async { Ok(vec![]) })
    }
    fn set_model<'a>(&'a self, _: &'a str) -> ClientControlOperation<'a> {
        Box::pin(async { Ok(()) })
    }
    fn interrupt(&self) -> ClientControlOperation<'_> {
        Box::pin(async { panic!("compaction must not interrupt a Turn") })
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

pub struct Fixture {
    pub state: AppState,
    pub channel: Arc<Channel>,
    pub root: tempfile::TempDir,
}
impl Fixture {
    pub async fn new(spec: &'static AgentClientSpec, rpc_history: bool) -> Self {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("state")).unwrap();
        let db = pontia_storage_sqlite::connect_sqlite("sqlite::memory:")
            .await
            .unwrap();
        pontia_storage_sqlite::run_migrations(&db).await.unwrap();
        let mut registration = test_registration();
        registration.spec = spec;
        registration.in_process = None;
        registration.data = Some(Arc::new(super::history::History { rpc: rpc_history }));
        let mut clients = ClientRegistry::default();
        clients.register(registration);
        let state = AppState::builder(db, root.path().into())
            .clients(clients)
            .external_api_token(Some("test-token".into()))
            .build();
        Self {
            state,
            channel: Arc::default(),
            root,
        }
    }
    pub async fn seed(&self) {
        for (id, kind) in [
            ("created", EventType::SessionCreated),
            ("ready", EventType::SessionReady),
        ] {
            self.state
                .event_ingest_service()
                .ingest_reported_event(ReportedEvent::new(
                    id.into(),
                    "session".into(),
                    None,
                    EventSource::AgentClient,
                    SPEC.client_type.into(),
                    kind,
                    json!({}),
                ))
                .await
                .unwrap();
        }
        sqlx::query("INSERT INTO session_runtimes(session_id,runtime_id,role,state,created_at) VALUES ('session','runtime','tui','running','2026-10-13T00:00:00Z')").execute(&self.state.db()).await.unwrap();
        sqlx::query("INSERT INTO agent_bindings(id,session_id,client_type,launch_cwd,client_session_key,metadata) VALUES ('binding','session','compaction-test',?,'native','{}')").bind(self.root.path().to_str().unwrap()).execute(&self.state.db()).await.unwrap();
        self.state
            .client_control()
            .attach(
                SPEC.client_type,
                "session",
                "runtime",
                "native",
                self.channel.clone(),
            )
            .await
            .unwrap();
    }
    pub async fn set_state(&self, state: &str) {
        sqlx::query("UPDATE sessions SET state=? WHERE session_id='session'")
            .bind(state)
            .execute(&self.state.db())
            .await
            .unwrap();
    }
    pub async fn request(
        &self,
        method: &str,
        uri: &str,
        key: Option<&str>,
        authenticated: bool,
    ) -> (StatusCode, Value) {
        request(self.state.clone(), method, uri, key, authenticated).await
    }
}

pub async fn request(
    state: AppState,
    method: &str,
    uri: &str,
    key: Option<&str>,
    authenticated: bool,
) -> (StatusCode, Value) {
    let mut request = Request::builder().method(method).uri(uri);
    if let Some(key) = key {
        request = request.header("Idempotency-Key", key);
    }
    if authenticated {
        request = request.header("Authorization", "Bearer test-token");
    }
    let response = pontia_http::router(state)
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}
