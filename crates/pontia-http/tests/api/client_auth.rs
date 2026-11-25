use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use axum::{
    Router,
    body::Body,
    http::{HeaderMap, Method, Request, StatusCode, header},
};
use ed25519_dalek::{Signer, SigningKey};
use http_body_util::BodyExt;
use pontia_agent_control::{
    AgentControl,
    client_contract::{
        ClientAuth, ClientOperation,
        auth::{
            AuthContext, AuthEventSink, AuthFlowHandle, AuthFlowUpdate, AuthMethodDescriptor,
            AuthMethodKind, AuthSessionState, AuthState, AuthStep, ClientAuthStatus,
            StartLoginParams,
        },
    },
    clients::ClientRegistry,
};
use pontia_application::AppState;
use pontia_core::Error;
use pontia_e2e::{
    BrowserIdentity, Capability, DeviceIdentity, DeviceSessions,
    bhttp::{Decoder, Encoder, Event, Head, Kind},
};
use pontia_http::{
    HttpEntrypoints,
    e2e::{CONTENT_TYPE, REQUESTS_PATH, SESSIONS_PATH},
};
use pontia_storage_sqlite::{
    connect_sqlite, repositories::agent_clients::SqliteAgentClientRepository, run_migrations,
};
use serde_json::Value;
use sqlx::SqlitePool;
use tower::ServiceExt;

const CODEX: &str = "/api/v1/agent-clients/codex/default/auth";
const KIRO: &str = "/api/v1/agent-clients/acp/kiro/auth";

#[derive(Default)]
struct MockAuth {
    states: Mutex<HashMap<String, AuthState>>,
    sinks: Mutex<HashMap<String, AuthEventSink>>,
    logouts: Mutex<Vec<(String, Option<String>)>>,
}

impl MockAuth {
    fn set_state(&self, key: &str, state: AuthState) {
        self.states.lock().unwrap().insert(key.into(), state);
    }

    fn sink(&self, key: &str) -> AuthEventSink {
        self.sinks.lock().unwrap()[key].clone()
    }
}

impl ClientAuth for MockAuth {
    fn supports_auth(&self, key: &str) -> bool {
        key != "antigravity"
    }

    fn status<'a>(
        &'a self,
        key: &'a str,
        _: &'a AuthContext,
    ) -> ClientOperation<'a, ClientAuthStatus> {
        Box::pin(async move {
            let state = self
                .states
                .lock()
                .unwrap()
                .get(key)
                .copied()
                .unwrap_or(AuthState::Unauthenticated);
            Ok(ClientAuthStatus::new(
                state,
                vec![AuthMethodDescriptor {
                    id: "device".into(),
                    label: "Device".into(),
                    description: None,
                    kind: AuthMethodKind::DeviceCode,
                    recommended: true,
                    remote_compatible: true,
                    start_fields: Vec::new(),
                }],
                true,
            ))
        })
    }

    fn start_login<'a>(
        &'a self,
        key: &'a str,
        method_id: &'a str,
        _: StartLoginParams,
        _: &'a AuthContext,
        sink: AuthEventSink,
    ) -> ClientOperation<'a, AuthFlowHandle> {
        Box::pin(async move {
            if method_id != "device" {
                return Err(Error::Domain(format!(
                    "Unsupported login method: {method_id}"
                )));
            }
            sink.emit(AuthFlowUpdate::Step {
                state: AuthSessionState::AwaitingUser,
                step: AuthStep::DeviceCode {
                    verification_url: "https://example.test/device".into(),
                    user_code: "ABCD-EFGH".into(),
                    expires_in_secs: Some(600),
                    message: None,
                },
                expires_in_secs: Some(600),
            })
            .await;
            self.sinks.lock().unwrap().insert(key.into(), sink);
            Ok(AuthFlowHandle {
                input: None,
                cancel: Box::new(|| Box::pin(async { Ok(()) })),
            })
        })
    }

    fn logout<'a>(
        &'a self,
        key: &'a str,
        scope: Option<&'a str>,
        _: &'a AuthContext,
    ) -> ClientOperation<'a, ()> {
        Box::pin(async move {
            self.logouts
                .lock()
                .unwrap()
                .push((key.into(), scope.map(str::to_owned)));
            self.set_state(key, AuthState::Unauthenticated);
            Ok(())
        })
    }
}

struct Harness {
    endpoints: HttpEntrypoints,
    router: Router,
    mock: Arc<MockAuth>,
    db: SqlitePool,
    _home: tempfile::TempDir,
}

async fn harness() -> Harness {
    let home = tempfile::tempdir().unwrap();
    let db = connect_sqlite("sqlite::memory:").await.unwrap();
    run_migrations(&db).await.unwrap();
    let mock = Arc::new(MockAuth::default());
    let mut codex = pontia_client_codex::registration();
    codex.auth = Some(mock.clone());
    let mut acp = pontia_client_acp::registration();
    acp.auth = Some(mock.clone());
    let mut clients = ClientRegistry::default();
    clients.register(codex);
    clients.register(acp);
    let state = AppState::builder(
        AgentControl::builder(db.clone(), home.path().into())
            .clients(clients)
            .build(),
    )
    .external_api_token(Some("test-token".into()))
    .build();
    let endpoints = HttpEntrypoints::new(state);
    Harness {
        router: endpoints.local_http(),
        endpoints,
        mock,
        db,
        _home: home,
    }
}

impl Harness {
    async fn call(&self, method: &str, path: &str, body: &str) -> (StatusCode, Value) {
        let response = self
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .header(header::AUTHORIZATION, "Bearer test-token")
                    .body(Body::from(body.to_owned()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        if path.contains("/auth") {
            assert_eq!(
                response.headers()[header::CACHE_CONTROL],
                "no-store",
                "{method} {path}"
            );
        }
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        (status, serde_json::from_slice(&bytes).unwrap())
    }

    async fn start(&self, base: &str) -> String {
        let (status, body) = self
            .call(
                "POST",
                &format!("{base}/sessions"),
                r#"{"method_id":"device"}"#,
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let id = body["data"]["session"]["session_id"]
            .as_str()
            .unwrap()
            .to_owned();
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let (_, body) = self.call("GET", &format!("{base}/sessions/{id}"), "").await;
                if body["data"]["session"]["state"] == "awaiting_user" {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        id
    }

    async fn stream(&self, path: &str) -> (StatusCode, Option<EventStream>) {
        let response = self
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .uri(path)
                    .header(header::AUTHORIZATION, "Bearer test-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        if status != StatusCode::OK {
            return (status, None);
        }
        let (sender, events) = tokio::sync::mpsc::unbounded_channel();
        let mut body = response.into_body();
        let reader = tokio::spawn(async move {
            let mut text = String::new();
            while let Some(Ok(frame)) = body.frame().await {
                let Ok(data) = frame.into_data() else {
                    continue;
                };
                text.push_str(std::str::from_utf8(&data).unwrap());
                while let Some(end) = text.find(
                    "

",
                ) {
                    let block: String = text.drain(..end + 2).collect();
                    if let Some(event) = parse_event(&block) {
                        let _ = sender.send(event);
                    }
                }
            }
        });
        (status, Some(EventStream { events, reader }))
    }
}

type SseEvent = (String, u64, Value);

struct EventStream {
    events: tokio::sync::mpsc::UnboundedReceiver<SseEvent>,
    reader: tokio::task::JoinHandle<()>,
}

impl EventStream {
    async fn next(&mut self) -> Option<SseEvent> {
        tokio::time::timeout(Duration::from_secs(5), self.events.recv())
            .await
            .expect("SSE event or end of stream")
    }

    /// Every remaining event; the server must close the stream.
    async fn rest(mut self) -> Vec<SseEvent> {
        let mut events = Vec::new();
        while let Some(event) = self.next().await {
            events.push(event);
        }
        tokio::time::timeout(Duration::from_secs(5), self.reader)
            .await
            .unwrap()
            .unwrap();
        events
    }
}

fn parse_event(block: &str) -> Option<(String, u64, Value)> {
    let mut name = None;
    let mut id = None;
    let mut data = None;
    for line in block.lines() {
        if let Some(value) = line.strip_prefix("event: ") {
            name = Some(value.to_owned());
        } else if let Some(value) = line.strip_prefix("id: ") {
            id = value.parse().ok();
        } else if let Some(value) = line.strip_prefix("data: ") {
            data = serde_json::from_str(value).ok();
        }
    }
    Some((name?, id?, data?))
}

#[tokio::test]
async fn auth_routes_report_status_conflicts_and_errors() {
    let harness = harness().await;
    let unauthorized = harness
        .router
        .clone()
        .oneshot(Request::builder().uri(CODEX).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);
    let (status, body) = harness.call("GET", CODEX, "").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["auth"]["state"], "unauthenticated");
    assert_eq!(body["data"]["auth"]["methods"][0]["kind"], "device_code");
    assert!(body["data"]["auth"].get("active_session_id").is_none());

    let (_, list) = harness.call("GET", "/api/v1/agent-clients", "").await;
    let supported: HashMap<String, bool> = list["data"]["agent_clients"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| {
            (
                format!(
                    "{}/{}",
                    entry["client_type"].as_str().unwrap(),
                    entry["client_key"].as_str().unwrap()
                ),
                entry["auth_supported"].as_bool().unwrap(),
            )
        })
        .collect();
    assert!(supported["codex/default"] && supported["acp/kiro"]);
    assert!(!supported["acp/antigravity"]);

    let unsupported = "/api/v1/agent-clients/acp/antigravity/auth";
    let (status, body) = harness.call("GET", unsupported, "").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["auth"]["state"], "unsupported");
    let (status, body) = harness
        .call(
            "POST",
            &format!("{unsupported}/sessions"),
            r#"{"method_id":"device"}"#,
        )
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"]["code"], "capability_unavailable");
    let (status, _) = harness
        .call("POST", &format!("{unsupported}/logout"), "")
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    for missing in [
        "/api/v1/agent-clients/missing/default/auth",
        "/api/v1/agent-clients/acp/missing/auth",
    ] {
        let (status, body) = harness.call("GET", missing, "").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["error"]["code"], "not_found");
    }

    for invalid in [
        "",
        "not json",
        r#"{"fields":{}}"#,
        r#"{"method_id":"device","fields":{"api_key":7}}"#,
        r#"{"method_id":"device","secret":"sk-leakedvalue123"}"#,
    ] {
        let (status, body) = harness
            .call("POST", &format!("{CODEX}/sessions"), invalid)
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{invalid}");
        assert_eq!(body["error"]["code"], "invalid_request");
        assert!(!body.to_string().contains("leaked"));
    }
    let (status, body) = harness
        .call(
            "POST",
            &format!("{CODEX}/sessions"),
            r#"{"method_id":"other"}"#,
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "invalid_request");

    let id = harness.start(CODEX).await;
    assert!(id.starts_with("auth_"));
    let (status, body) = harness
        .call(
            "POST",
            &format!("{CODEX}/sessions"),
            r#"{"method_id":"device"}"#,
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "auth_session_in_progress");
    let (_, body) = harness.call("GET", CODEX, "").await;
    assert_eq!(body["data"]["auth"]["active_session_id"], id.as_str());
    harness.start(KIRO).await;

    let (status, body) = harness
        .call("GET", &format!("{CODEX}/sessions/{id}"), "")
        .await;
    assert_eq!(status, StatusCode::OK);
    let session = &body["data"]["session"];
    assert_eq!(session["client_type"], "codex");
    assert_eq!(session["step"]["type"], "device_code");
    assert_eq!(session["step"]["user_code"], "ABCD-EFGH");
    for path in [
        format!("{KIRO}/sessions/{id}"),
        format!("{CODEX}/sessions/auth_missing"),
    ] {
        let (status, body) = harness.call("GET", &path, "").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["error"]["code"], "not_found");
    }
    let (status, _) = harness
        .call("POST", &format!("{KIRO}/sessions/{id}/cancel"), "")
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let input = format!("{CODEX}/sessions/{id}/input");
    let (status, body) = harness
        .call(
            "POST",
            &input,
            r#"{"type":"auth_code","code":"secret-code"}"#,
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "auth_input_not_expected");
    let (status, body) = harness
        .call(
            "POST",
            &input,
            r#"{"type":"auth_code","code":"secret-code","x":1}"#,
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(!body.to_string().contains("secret-code"));

    let (status, body) = harness.call("POST", &format!("{CODEX}/logout"), "").await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "auth_session_in_progress");

    let cancel = format!("{CODEX}/sessions/{id}/cancel");
    let (status, body) = harness.call("POST", &cancel, "{}").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["session"]["state"], "cancelled");
    assert!(body["data"]["session"]["terminal_at"].is_string());
    let (status, body) = harness.call("POST", &cancel, "").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["session"]["state"], "cancelled");
    let (status, body) = harness.call("POST", &input, r#"{"type":"confirm"}"#).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "auth_session_already_settled");
}

#[tokio::test]
async fn logout_clears_the_model_catalog_and_returns_fresh_status() {
    let harness = harness().await;
    let repository = SqliteAgentClientRepository::new(harness.db.clone());
    repository.save("codex", "default", true).await.unwrap();
    repository
        .save_model_catalog("codex", "default", None, r#"{"fetched_at":1}"#, 1)
        .await
        .unwrap();
    // Disabled entries skip the post-login model refresh, which would reach a real client.
    repository.save("codex", "default", false).await.unwrap();
    harness.mock.set_state("default", AuthState::Authenticated);
    let (status, body) = harness.call("POST", &format!("{CODEX}/logout"), "").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["auth"]["state"], "unauthenticated");
    let (status, _) = harness
        .call("POST", &format!("{CODEX}/logout"), r#"{"scope":"openai"}"#)
        .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = harness
        .call("POST", &format!("{CODEX}/logout"), r#"{"other":1}"#)
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        *harness.mock.logouts.lock().unwrap(),
        [
            ("default".to_owned(), None),
            ("default".to_owned(), Some("openai".to_owned()))
        ]
    );
    let catalog: Option<String> =
        sqlx::query_scalar("SELECT model_catalog FROM agent_clients WHERE client_type='codex'")
            .fetch_one(&harness.db)
            .await
            .unwrap();
    assert_eq!(catalog, None);
    let row = repository.get("codex", "default").await.unwrap().unwrap();
    assert!(!row.enabled);
}

#[tokio::test]
async fn stream_sends_snapshot_updates_and_terminal() {
    let harness = harness().await;
    let repository = SqliteAgentClientRepository::new(harness.db.clone());
    repository.save("codex", "default", true).await.unwrap();
    repository
        .save_model_catalog("codex", "default", None, r#"{"fetched_at":1}"#, 1)
        .await
        .unwrap();
    repository.save("codex", "default", false).await.unwrap();
    let id = harness.start(CODEX).await;
    let path = format!("{CODEX}/sessions/{id}/stream");
    let (status, stream) = harness.stream(&path).await;
    assert_eq!(status, StatusCode::OK);
    let mut stream = stream.unwrap();
    let first = stream.next().await.unwrap();
    assert_eq!(first.0, "snapshot");
    assert_eq!(first.2["state"], "awaiting_user");
    assert_eq!(first.1, first.2["sequence"].as_u64().unwrap());

    harness.mock.set_state("default", AuthState::Authenticated);
    harness
        .mock
        .sink("default")
        .emit(AuthFlowUpdate::Completed)
        .await;
    let rest = stream.rest().await;
    let names: Vec<&str> = rest.iter().map(|(name, _, _)| name.as_str()).collect();
    assert_eq!(names, ["updated", "terminal"]);
    assert_eq!(rest[0].2["state"], "completing");
    let terminal = &rest[1].2;
    assert_eq!(terminal["state"], "succeeded");
    assert_eq!(terminal["step"]["type"], "succeeded");
    assert_eq!(terminal["step"]["status"]["state"], "authenticated");
    assert!(first.1 < rest[0].1 && rest[0].1 < rest[1].1);
    let catalog: Option<String> =
        sqlx::query_scalar("SELECT model_catalog FROM agent_clients WHERE client_type='codex'")
            .fetch_one(&harness.db)
            .await
            .unwrap();
    assert_eq!(catalog, None);
    let row = repository.get("codex", "default").await.unwrap().unwrap();
    assert!(!row.enabled, "login does not change enablement");

    let (status, stream) = harness.stream(&path).await;
    assert_eq!(status, StatusCode::OK);
    let settled = stream.unwrap().rest().await;
    assert_eq!(settled.len(), 1);
    assert_eq!(settled[0].0, "snapshot");
    assert_eq!(settled[0].2["state"], "succeeded");

    let (status, _) = harness
        .stream(&format!("{KIRO}/sessions/{id}/stream"))
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

struct E2e {
    ingress: pontia_http::e2e::E2eIngress,
    session: pontia_e2e::BrowserSession,
}

impl E2e {
    async fn new(endpoints: &HttpEntrypoints) -> Self {
        let identity = DeviceIdentity::from_private_bytes([7; 16], 1, &[11; 32]).unwrap();
        let public = identity.public_key();
        let signer = SigningKey::from_bytes(&[9; 32]);
        let ingress = endpoints.e2e_tunnel(DeviceSessions::new(identity, signer.verifying_key()));
        let browser = BrowserIdentity::generate().unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let mut capability = vec![1];
        capability.extend_from_slice(&[7; 16]);
        capability.extend_from_slice(&1u64.to_be_bytes());
        capability.extend_from_slice(&browser.public_key());
        capability.extend_from_slice(&[13; 32]);
        capability.extend_from_slice(&now.to_be_bytes());
        capability.extend_from_slice(&(now + 60).to_be_bytes());
        capability.extend_from_slice(&[0; 64]);
        let signed = signer.sign(&Capability::decode(&capability).unwrap().signing_bytes());
        capability[105..].copy_from_slice(&signed.to_bytes());
        let handshake = browser.start(&public, &capability).unwrap();
        let confirmation = ingress
            .handle(outer(SESSIONS_PATH, handshake.request_bytes().to_vec()))
            .await;
        assert_eq!(confirmation.status(), StatusCode::OK);
        let confirmation = confirmation.into_body().collect().await.unwrap().to_bytes();
        let session = handshake.confirm(&confirmation).unwrap();
        Self { ingress, session }
    }

    async fn get(&self, path: &str) -> (u16, HeaderMap, Vec<u8>) {
        let (context, mut upload, mut download) = self.session.request().unwrap();
        let (mut bhttp, head) = Encoder::new(&Head::Request {
            method: Method::GET,
            uri: path.parse().unwrap(),
            headers: HeaderMap::new(),
        })
        .unwrap();
        let mut wire = vec![1];
        wire.extend_from_slice(&context.session_id);
        wire.extend_from_slice(&context.request_id);
        wire.extend_from_slice(&upload.seal(&head).unwrap());
        wire.extend_from_slice(&upload.seal(&bhttp.finish().unwrap()).unwrap());
        wire.extend_from_slice(&upload.finish().unwrap());
        let response = self.ingress.handle(outer(REQUESTS_PATH, wire)).await;
        assert_eq!(response.status(), StatusCode::OK);
        let encrypted = response.into_body().collect().await.unwrap().to_bytes();
        let mut remaining = encrypted.as_ref();
        let mut bhttp = Decoder::new(Kind::Response);
        let mut head = None;
        let mut content = Vec::new();
        while !remaining.is_empty() {
            let (used, plaintext) = download.feed(remaining).unwrap();
            remaining = &remaining[used..];
            let Some(plaintext) = plaintext else { continue };
            let mut remaining = plaintext.as_slice();
            while !remaining.is_empty() {
                let (used, event) = bhttp.feed(remaining).unwrap();
                remaining = &remaining[used..];
                match event {
                    Some(Event::Head(Head::Response { status, headers })) => {
                        head = Some((status, headers))
                    }
                    Some(Event::Content(bytes)) => content.extend_from_slice(&bytes),
                    _ => {}
                }
            }
        }
        download.finish().unwrap();
        bhttp.finish().unwrap();
        let (status, headers) = head.unwrap();
        (status, headers, content)
    }
}

fn outer(path: &str, wire: Vec<u8>) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(path)
        .header(header::CONTENT_TYPE, CONTENT_TYPE)
        .body(Body::from(wire))
        .unwrap()
}

#[tokio::test]
async fn auth_routes_work_through_the_e2e_session() {
    let harness = harness().await;
    let e2e = E2e::new(&harness.endpoints).await;
    let (status, headers, content) = e2e.get(CODEX).await;
    assert_eq!(status, 200);
    assert_eq!(headers[header::CACHE_CONTROL], "no-store");
    let remote: Value = serde_json::from_slice(&content).unwrap();
    assert_eq!(remote["data"]["auth"]["state"], "unauthenticated");

    let id = harness.start(CODEX).await;
    harness
        .call("POST", &format!("{CODEX}/sessions/{id}/cancel"), "")
        .await;
    let (status, headers, content) = e2e.get(&format!("{CODEX}/sessions/{id}/stream")).await;
    assert_eq!(status, 200);
    assert_eq!(headers[header::CONTENT_TYPE], "text/event-stream");
    let text = String::from_utf8(content).unwrap();
    let event = parse_event(&text).unwrap();
    assert_eq!(event.0, "snapshot");
    assert_eq!(event.2["state"], "cancelled");
}
