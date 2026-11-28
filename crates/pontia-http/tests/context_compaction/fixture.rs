use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use pontia_agent_control::{
    DeliveryOperation, EventIngestService, InputPayload,
    client_contract::{
        AgentClientCapabilities, AgentClientSpec, ClientExitMode, ClientExitOutcome,
        ClientOperation, ClientSession, ClientSessionDetails, DispatchBehavior,
        RuntimeBindingBehavior, TEST_SPEC, native_history::NativeHistory,
        raw_transcripts::AgentBindingResolveRequest, test_registration,
    },
    clients::ClientRegistry,
    control::InputReceipt,
    runtime::ControlTarget,
};
use pontia_application::AppState;
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
    adapter: pontia_agent_control::client_contract::AgentClientAdapter {
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
    adapter: pontia_agent_control::client_contract::AgentClientAdapter {
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
    pub generation: std::sync::atomic::AtomicUsize,
    pub reads: Arc<std::sync::atomic::AtomicUsize>,
    pub rpc_history: bool,
}
impl ClientSession for Channel {
    fn provision<'a>(
        &'a self,
        _events: EventIngestService,
        _root: &'a std::path::Path,
        _request: pontia_runtime::RuntimeStartRequest,
    ) -> ClientOperation<'a, Option<pontia_runtime::RuntimeStartResult>> {
        Box::pin(async { Ok(None) })
    }
    fn compact<'a>(
        &'a self,
        _events: EventIngestService,
        _target: &'a ControlTarget,
        operation_id: &'a str,
    ) -> ClientOperation<'a, ()> {
        Box::pin(async move {
            let generation = self.generation.load(Ordering::SeqCst);
            self.operations.lock().unwrap().push(operation_id.into());
            if self.delayed.load(Ordering::SeqCst) {
                self.started.notify_one();
                self.finish.notified().await;
            }
            if self.generation.load(Ordering::SeqCst) != generation {
                return Err(Error::ControlUnknown("connection replaced".into()));
            }
            match self.error.lock().unwrap().take() {
                Some(error) => Err(error),
                None => Ok(()),
            }
        })
    }
    fn interrupt<'a>(
        &'a self,
        _events: EventIngestService,
        _target: &'a ControlTarget,
    ) -> DeliveryOperation<'a> {
        Box::pin(async {
            self.operations.lock().unwrap().push("interrupt".into());
            Ok(())
        })
    }
    fn exit<'a>(
        &'a self,
        _events: EventIngestService,
        _target: &'a ControlTarget,
        _mode: ClientExitMode,
    ) -> ClientOperation<'a, ClientExitOutcome> {
        Box::pin(async { Ok(ClientExitOutcome::Requested) })
    }
    fn start_turn<'a>(
        &'a self,
        _events: EventIngestService,
        target: &'a ControlTarget,
        payload: &'a InputPayload,
        _turn: Option<&'a str>,
    ) -> DeliveryOperation<'a, InputReceipt> {
        Box::pin(async move {
            self.operations.lock().unwrap().push(payload.text.clone());
            Ok(InputReceipt {
                turn_pending: false,
                native_turn_id: None,
                runtime_id: target.runtime_id.clone(),
            })
        })
    }
    fn native_history(
        &self,
        _store: pontia_agent_control::ClientControlStore,
        _binding: AgentBindingResolveRequest,
    ) -> Option<Arc<dyn NativeHistory>> {
        self.rpc_history.then(|| {
            Arc::new(super::history::NativeReader(self.reads.clone())) as Arc<dyn NativeHistory>
        })
    }
    fn available<'a>(
        &'a self,
        _store: pontia_agent_control::ClientControlStore,
        _session: &'a str,
    ) -> ClientOperation<'a, bool> {
        Box::pin(async move { Ok(!self.closed.load(Ordering::SeqCst)) })
    }
    fn details<'a>(
        &'a self,
        _store: pontia_agent_control::ClientControlStore,
        _session: &'a str,
    ) -> ClientOperation<'a, ClientSessionDetails> {
        Box::pin(async { Ok(ClientSessionDetails { data: json!({}) }) })
    }
}

pub struct Fixture {
    pub reads: Arc<std::sync::atomic::AtomicUsize>,
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
        let reads = Arc::default();
        let channel = Arc::new(Channel {
            reads: Arc::clone(&reads),
            rpc_history,
            ..Default::default()
        });
        registration.session = Some(channel.clone());
        registration.data = Some(Arc::new(super::history::History { rpc: rpc_history }));
        let mut clients = ClientRegistry::default();
        clients.register(registration);
        let state = AppState::builder(
            pontia_agent_control::AgentControl::builder(db, root.path().into())
                .clients(clients)
                .build(),
        )
        .external_api_token(Some("test-token".into()))
        .build();
        Self {
            state,
            channel,
            reads,
            root,
        }
    }
    pub async fn seed(&self) {
        for (id, kind) in [
            ("created", EventType::SessionCreated),
            ("ready", EventType::SessionReady),
        ] {
            self.state
                .control()
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
        sqlx::query("INSERT INTO session_runtimes(session_id,runtime_id,role,state,created_at) VALUES ('session','runtime','generic_control','running','2026-10-13T00:00:00Z')").execute(&self.state.db()).await.unwrap();
        sqlx::query("INSERT INTO agent_bindings(id,session_id,client_type,launch_cwd,client_session_key,metadata) VALUES ('binding','session','compaction-test',?,'native','{}')").bind(self.root.path().to_str().unwrap()).execute(&self.state.db()).await.unwrap();
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
