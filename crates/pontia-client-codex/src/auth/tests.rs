use super::*;
use futures_util::{SinkExt, StreamExt};
use pontia_agent_control::client_contract::auth::AuthErrorCode;
use std::collections::HashMap;
use tokio::{
    net::UnixListener,
    sync::{Mutex, mpsc},
};
use tokio_tungstenite::{accept_async, tungstenite::Message};

#[derive(Default)]
struct Server {
    account: Mutex<Value>,
    requests: Mutex<Vec<Value>>,
    peers: Mutex<Vec<mpsc::UnboundedSender<Option<Value>>>>,
}

impl Server {
    async fn notify(&self, event: Value) {
        for peer in self.peers.lock().await.iter() {
            let _ = peer.send(Some(event.clone()));
        }
    }

    async fn disconnect(&self) {
        for peer in self.peers.lock().await.drain(..) {
            let _ = peer.send(None);
        }
    }

    async fn requests(&self, method: &str) -> Vec<Value> {
        self.requests
            .lock()
            .await
            .iter()
            .filter(|request| request["method"] == method)
            .cloned()
            .collect()
    }
}

struct Fixture {
    root: tempfile::TempDir,
    server: Arc<Server>,
}

impl Fixture {
    async fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let socket = root.path().join("app-server.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let server = Arc::new(Server::default());
        *server.account.lock().await = json!({"account": null, "requiresOpenaiAuth": true});
        let shared = server.clone();
        let home = root.path().canonicalize().unwrap();
        tokio::spawn(async move {
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                let shared = shared.clone();
                let home = home.clone();
                tokio::spawn(async move {
                    let mut wire = accept_async(stream).await.unwrap();
                    let (sender, mut commands) = mpsc::unbounded_channel();
                    shared.peers.lock().await.push(sender);
                    loop {
                        tokio::select! {
                            command = commands.recv() => match command {
                                Some(Some(event)) => {
                                    if wire.send(Message::Text(event.to_string().into())).await.is_err() { break; }
                                }
                                _ => break,
                            },
                            frame = wire.next() => {
                                let Some(Ok(Message::Text(frame))) = frame else { break };
                                let request: Value = serde_json::from_str(&frame).unwrap();
                                let Some(id) = request.get("id").cloned() else { continue };
                                shared.requests.lock().await.push(request.clone());
                                let result = match request["method"].as_str().unwrap() {
                                    "initialize" => json!({"userAgent":"codex/test","codexHome":home,"platformFamily":"unix","platformOs":std::env::consts::OS}),
                                    "account/read" => shared.account.lock().await.clone(),
                                    "account/login/start" => json!({"type":"chatgptDeviceCode","loginId":"login-1","verificationUrl":"https://auth.openai.com/codex/device","userCode":"ABCD-EFGH"}),
                                    "account/login/cancel" => json!({"status":"canceled"}),
                                    "account/logout" => json!({}),
                                    method => panic!("unexpected RPC {method}"),
                                };
                                if wire.send(Message::Text(json!({"id":id,"result":result}).to_string().into())).await.is_err() { break; }
                            }
                        }
                    }
                });
            }
        });
        CodexRuntime::install_for_test(root.path(), socket)
            .await
            .unwrap();
        Self { root, server }
    }

    fn context(&self) -> AuthContext {
        AuthContext {
            pontia_home: self.root.path().to_path_buf(),
            vars: Arc::new(HashMap::new()),
        }
    }

    async fn start(&self) -> (AuthFlowHandle, mpsc::Receiver<AuthFlowUpdate>) {
        let (sender, mut updates) = mpsc::channel(8);
        let handle = CodexAuth
            .start_login(
                "default",
                DEVICE_CODE,
                StartLoginParams::default(),
                &self.context(),
                AuthEventSink::new(sender),
            )
            .await
            .unwrap();
        let Some(AuthFlowUpdate::Step {
            state: AuthSessionState::AwaitingUser,
            step:
                AuthStep::DeviceCode {
                    verification_url,
                    user_code,
                    expires_in_secs: Some(900),
                    ..
                },
            expires_in_secs: Some(900),
        }) = updates.recv().await
        else {
            panic!("expected a device code step");
        };
        assert_eq!(verification_url, "https://auth.openai.com/codex/device");
        assert_eq!(user_code, "ABCD-EFGH");
        (handle, updates)
    }
}

async fn next(updates: &mut mpsc::Receiver<AuthFlowUpdate>) -> AuthFlowUpdate {
    tokio::time::timeout(Duration::from_secs(5), updates.recv())
        .await
        .unwrap()
        .unwrap()
}

#[test]
fn account_responses_map_to_status() {
    let unauthenticated = status_from_account(&json!({"account":null,"requiresOpenaiAuth":true}));
    assert_eq!(unauthenticated.state, AuthState::Unauthenticated);
    assert_eq!(unauthenticated.method_id, None);
    let not_required = status_from_account(&json!({"account":null,"requiresOpenaiAuth":false}));
    assert_eq!(not_required.state, AuthState::Authenticated);
    assert!(not_required.detail.is_some());
    let chatgpt = status_from_account(&json!({"account":{
        "type":"chatgpt","email":"user@example.com","planType":"plus","workspaceName":"Personal"
    },"requiresOpenaiAuth":true}));
    assert_eq!(chatgpt.state, AuthState::Authenticated);
    assert_eq!(chatgpt.method_id.as_deref(), Some(DEVICE_CODE));
    assert_eq!(
        chatgpt.account,
        Some(AuthAccountInfo {
            label: Some("user@example.com".into()),
            plan: Some("plus".into()),
            provider: Some("openai".into()),
            workspace: Some("Personal".into()),
            expires_at: None,
        })
    );
    let api_key = status_from_account(&json!({"account":{"type":"apiKey"}}));
    assert_eq!(api_key.method_id.as_deref(), Some("api_key"));
    assert_eq!(api_key.account.unwrap().label.as_deref(), Some("API Key"));
    let bedrock = status_from_account(&json!({"account":{"type":"amazonBedrock"}}));
    assert_eq!(bedrock.state, AuthState::Authenticated);
    assert_eq!(
        bedrock.account.unwrap().provider.as_deref(),
        Some("amazonBedrock")
    );
    assert_eq!(chatgpt.methods.len(), 1);
    assert_eq!(chatgpt.methods[0].kind, AuthMethodKind::DeviceCode);
    assert!(chatgpt.supports_logout);
}

#[tokio::test]
async fn status_reads_the_account_without_refreshing_tokens() {
    let fixture = Fixture::new().await;
    let status = CodexAuth
        .status("default", &fixture.context())
        .await
        .unwrap();
    assert_eq!(status.state, AuthState::Unauthenticated);
    *fixture.server.account.lock().await =
        json!({"account":{"type":"chatgpt","email":"user@example.com","planType":"pro"}});
    let status = CodexAuth
        .status("default", &fixture.context())
        .await
        .unwrap();
    assert_eq!(status.state, AuthState::Authenticated);
    let reads = fixture.server.requests("account/read").await;
    assert!(
        reads
            .iter()
            .all(|read| read["params"] == json!({"refreshToken": false}))
    );
}

#[tokio::test]
async fn device_code_login_completes_on_the_matching_notification() {
    let fixture = Fixture::new().await;
    let (_handle, mut updates) = fixture.start().await;
    assert_eq!(
        fixture.server.requests("account/login/start").await[0]["params"],
        json!({"type":"chatgptDeviceCode"})
    );
    fixture
        .server
        .notify(
            json!({"method":"account/login/completed","params":{"loginId":"other","success":true}}),
        )
        .await;
    fixture
        .server
        .notify(json!({"method":"account/login/completed","params":{"loginId":"login-1","success":true}}))
        .await;
    assert!(matches!(
        next(&mut updates).await,
        AuthFlowUpdate::Completed
    ));
}

#[tokio::test]
async fn rejected_login_reports_a_sanitized_failure() {
    let fixture = Fixture::new().await;
    let (_handle, mut updates) = fixture.start().await;
    fixture
        .server
        .notify(json!({"method":"account/login/completed","params":{"loginId":"login-1","success":false,"error":"denied ?code=secret"}}))
        .await;
    let AuthFlowUpdate::Failed {
        code: AuthErrorCode::ProviderRejected,
        message,
        retryable: true,
    } = next(&mut updates).await
    else {
        panic!("expected failure");
    };
    assert_eq!(message, "denied ?code=[REDACTED]");
}

#[tokio::test]
async fn cancel_cancels_the_native_login() {
    let fixture = Fixture::new().await;
    let (handle, _updates) = fixture.start().await;
    (handle.cancel)().await.unwrap();
    assert_eq!(
        fixture.server.requests("account/login/cancel").await[0]["params"],
        json!({"loginId":"login-1"})
    );
}

#[tokio::test]
async fn disconnect_fails_the_login() {
    let fixture = Fixture::new().await;
    let (_handle, mut updates) = fixture.start().await;
    fixture.server.disconnect().await;
    assert!(matches!(
        next(&mut updates).await,
        AuthFlowUpdate::Failed {
            code: AuthErrorCode::ClientUnavailable,
            ..
        }
    ));
}

#[tokio::test]
async fn logout_and_method_validation() {
    let fixture = Fixture::new().await;
    CodexAuth
        .logout("default", None, &fixture.context())
        .await
        .unwrap();
    assert_eq!(
        fixture.server.requests("account/logout").await[0]["params"],
        Value::Null
    );
    let (sender, _updates) = mpsc::channel(1);
    let result = CodexAuth
        .start_login(
            "default",
            "api_key",
            StartLoginParams::default(),
            &fixture.context(),
            AuthEventSink::new(sender),
        )
        .await;
    assert!(matches!(result, Err(Error::Domain(_))));
}
