use crate::{
    CodexObserver,
    runtime::{CodexRuntime, SubscriptionState},
};
use futures_util::{SinkExt, StreamExt};
use pontia_application::{
    AgentBindingService, AppState, UpsertAgentBindingRequest,
    clients::ClientRegistry,
    interactions::{Interaction, InteractionState},
};
use serde_json::{Value, json};
use std::time::Duration;
use tokio::{
    net::UnixListener,
    sync::{mpsc, watch},
    task::JoinHandle,
};
use tokio_tungstenite::{accept_async, tungstenite::Message};

struct Fixture {
    app: AppState,
    session: String,
    root: tempfile::TempDir,
    commands: mpsc::UnboundedSender<Value>,
    replies: mpsc::UnboundedReceiver<Value>,
    server: JoinHandle<()>,
    observer: JoinHandle<()>,
    shutdown: watch::Sender<bool>,
}
impl Fixture {
    async fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let socket = root.path().join("app-server.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let (commands, mut incoming) = mpsc::unbounded_channel::<Value>();
        let (responses, replies) = mpsc::unbounded_channel();
        let home = root.path().canonicalize().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut wire = accept_async(stream).await.unwrap();
            loop {
                tokio::select! {
                    value = incoming.recv() => {
                        let Some(value) = value else { break };
                        if value.is_null() { wire.close(None).await.unwrap(); break; }
                        wire.send(Message::Text(value.to_string().into())).await.unwrap();
                    }
                    frame = wire.next() => {
                        let Some(Ok(Message::Text(text))) = frame else { break };
                        let request: Value = serde_json::from_str(&text).unwrap();
                        if request.get("method").is_none() {
                            responses.send(request.clone()).unwrap();
                            wire.send(Message::Text(json!({"method":"serverRequest/resolved","params":{"threadId":"thread","requestId":request["id"]}}).to_string().into())).await.unwrap();
                            continue;
                        }
                        let Some(id) = request.get("id") else { continue };
                        let thread = json!({"id":"thread","cwd":home,"status":{"type":"active"},"canAcceptDirectInput":true});
                        let result = match request["method"].as_str().unwrap() {
                            "initialize" => json!({"codexHome":home,"platformFamily":"unix","platformOs":std::env::consts::OS,"userAgent":"codex/test"}),
                            "thread/resume" | "thread/read" => json!({"thread":thread,"model":"test"}),
                            "thread/unsubscribe" => json!({"status":"unsubscribed"}),
                            "thread/turns/list" => json!({"data":[{"id":"native-turn","status":"inProgress","items":[{"id":"user","type":"userMessage","content":[{"type":"text","text":"hello"}]}]}],"nextCursor":null}),
                            _ => panic!("Unexpected request: {request}"),
                        };
                        wire.send(Message::Text(json!({"id":id,"result":result}).to_string().into())).await.unwrap();
                    }
                }
            }
        });
        let runtime = CodexRuntime::install_for_test(root.path(), socket)
            .await
            .unwrap();
        let pool = pontia_storage_sqlite::connect_sqlite("sqlite::memory:")
            .await
            .unwrap();
        pontia_storage_sqlite::run_migrations(&pool).await.unwrap();
        let mut clients = ClientRegistry::default();
        clients.register(crate::registration());
        let app = AppState::builder(pool, root.path().into())
            .clients(clients)
            .build();
        let session = app
            .session_commands()
            .create_session(
                serde_json::from_value(json!({"client_type":"codex","workspace":root.path()}))
                    .unwrap(),
            )
            .await
            .unwrap()
            .session_id()
            .unwrap()
            .to_owned();
        AgentBindingService::new(app.db())
            .upsert_binding(UpsertAgentBindingRequest {
                session_id: session.clone(),
                client_type: "codex".into(),
                launch_cwd: root.path().display().to_string(),
                client_session_key: "thread".into(),
                client_session_file: None,
                metadata: json!({}),
            })
            .await
            .unwrap();
        let (shutdown, rx) = watch::channel(false);
        let observer = CodexObserver::new(&app, root.path().into());
        observer.prepare().await.unwrap();
        let observer = tokio::spawn(observer.run(rx));
        tokio::time::timeout(Duration::from_secs(5), async {
            while runtime.subscription(&session).await != Some(SubscriptionState::Available) {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        Self {
            app,
            session,
            root,
            commands,
            replies,
            server,
            observer,
            shutdown,
        }
    }
    fn send(&self, value: Value) {
        self.commands.send(value).unwrap();
    }
    async fn wait(&self, count: usize, state: InteractionState) -> Vec<Interaction> {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let items = self
                    .app
                    .interactions()
                    .list(&self.session, Some(state.clone()), 200, None)
                    .await
                    .unwrap()
                    .interactions;
                if items.len() == count {
                    return items;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap()
    }
    async fn response(&mut self) -> Value {
        tokio::time::timeout(Duration::from_secs(5), self.replies.recv())
            .await
            .unwrap()
            .unwrap()
    }
    async fn stop(self) {
        let _ = self.shutdown.send(true);
        self.observer.await.unwrap();
        self.app.inbox_commands().stop_scheduling().await;
        CodexRuntime::shutdown(self.root.path()).await;
        self.server.abort();
        let _ = self.server.await;
    }
}
fn request(method: &str, id: Value) -> Value {
    let legacy = matches!(method, "execCommandApproval" | "applyPatchApproval");
    let mut params = json!({"itemId":"same-item","reason":"<script>untrusted</script>","command":"echo hello","permissions":{"network":{"enabled":true},"fileSystem":null},"grantRoot":"/not/a/file/list"});
    if legacy {
        params["conversationId"] = json!("thread");
        params["callId"] = json!("not-a-turn");
        params["command"] = json!(["echo", "hello"]);
    } else {
        params["threadId"] = json!("thread");
        params["turnId"] = json!("native-turn");
    }
    json!({"method":method,"id":id,"params":params})
}

#[tokio::test]
async fn every_approval_maps_both_choices_to_a_single_native_reply() {
    let mut f = Fixture::new().await;
    let cases = [
        (
            "item/commandExecution/requestApproval",
            json!({"decision":"accept"}),
            json!({"decision":"decline"}),
        ),
        (
            "item/fileChange/requestApproval",
            json!({"decision":"accept"}),
            json!({"decision":"decline"}),
        ),
        (
            "item/permissions/requestApproval",
            json!({"permissions":{"network":{"enabled":true}},"scope":"turn"}),
            json!({"permissions":{},"scope":"turn"}),
        ),
        (
            "execCommandApproval",
            json!({"decision":"approved"}),
            json!({"decision":"denied"}),
        ),
        (
            "applyPatchApproval",
            json!({"decision":"approved"}),
            json!({"decision":"denied"}),
        ),
    ];
    for (index, (method, allow, deny)) in cases.into_iter().enumerate() {
        for (choice, expected) in [("allow", allow), ("deny", deny)] {
            let id = if choice == "allow" {
                json!(format!("server-{index}"))
            } else {
                json!(index as u64 + 100)
            };
            f.send(request(method, id.clone()));
            let interaction = f.wait(1, InteractionState::Pending).await.remove(0);
            assert_eq!(interaction.session_id, f.session);
            if method.starts_with("item/") {
                assert!(interaction.turn_id.is_some());
            } else {
                assert!(interaction.turn_id.is_none());
            }
            let resolved = f
                .app
                .interactions()
                .respond(&f.app, &f.session, &interaction.interaction_id, choice)
                .await
                .unwrap();
            assert_eq!(resolved.state, InteractionState::Resolved);
            let response = f.response().await;
            assert_eq!(response, json!({"id":id,"result":expected}));
            // Our own resolved notification must leave persisted completion intact.
            let duplicate = f
                .app
                .interactions()
                .respond(&f.app, &f.session, &interaction.interaction_id, "anything")
                .await
                .unwrap();
            assert_eq!(duplicate, resolved);
        }
    }
    assert!(f.replies.try_recv().is_err());
    f.stop().await;
}

#[tokio::test]
async fn independent_requests_do_not_block_the_socket_reader_or_merge_by_item() {
    let mut f = Fixture::new().await;
    f.send(request(
        "item/commandExecution/requestApproval",
        json!("one"),
    ));
    f.send(request(
        "item/commandExecution/requestApproval",
        json!("two"),
    ));
    let interactions = f.wait(2, InteractionState::Pending).await;
    assert_ne!(
        interactions[0].interaction_id,
        interactions[1].interaction_id
    );
    let runtime = CodexRuntime::existing(f.root.path()).await.unwrap();
    let connection = runtime.connection().await.unwrap();
    let snapshot = connection
        .call("thread/read", json!({"threadId":"thread"}))
        .await
        .unwrap();
    assert_eq!(snapshot["thread"]["id"], "thread");
    for interaction in interactions.iter().rev() {
        f.app
            .interactions()
            .respond(&f.app, &f.session, &interaction.interaction_id, "deny")
            .await
            .unwrap();
        assert_eq!(f.response().await["result"]["decision"], "decline");
    }
    f.stop().await;
}

#[tokio::test]
async fn external_resolution_turn_end_and_disconnect_expire_native_requests() {
    let f = Fixture::new().await;
    f.send(request("item/fileChange/requestApproval", json!(1)));
    let i = f.wait(1, InteractionState::Pending).await.remove(0);
    f.send(json!({"method":"serverRequest/resolved","params":{"threadId":"thread","requestId":1}}));
    f.wait(1, InteractionState::Expired).await;
    assert!(
        f.app
            .interactions()
            .respond(&f.app, &f.session, &i.interaction_id, "allow")
            .await
            .is_err()
    );
    f.send(request("execCommandApproval", json!("legacy")));
    f.wait(1, InteractionState::Pending).await;
    f.send(json!({"method":"turn/completed","params":{"threadId":"thread","turn":{"id":"native-turn","status":"completed","items":[]}}}));
    f.wait(2, InteractionState::Expired).await;
    f.send(request("item/permissions/requestApproval", json!(3)));
    f.wait(1, InteractionState::Pending).await;
    f.send(Value::Null);
    f.wait(3, InteractionState::Expired).await;
    f.stop().await;
}

#[tokio::test]
async fn lag_keeps_valid_requests_and_discovers_approvals_lost_from_the_event_stream() {
    let mut f = Fixture::new().await;
    f.send(request("item/commandExecution/requestApproval", json!(1)));
    f.wait(1, InteractionState::Pending).await;
    let runtime = CodexRuntime::existing(f.root.path()).await.unwrap();
    let connection = runtime.connection().await.unwrap();
    let gate = connection.interactions.lock().await;
    f.send(request("item/fileChange/requestApproval", json!(2)));
    f.send(request("item/permissions/requestApproval", json!(3)));
    tokio::time::timeout(Duration::from_secs(5), async {
        while connection.pending_server_requests().await.len() != 3 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    // A stalled observer loses broadcast history, while the reader retains live
    // native requests on this same connection. No canonical states live here.
    for sequence in 1..=8192 {
        let _ = connection
            .events
            .send(crate::runtime::protocol::Notification {
                value: json!({"method":"ignored/output"}),
                sequence,
            });
    }
    drop(gate);
    let interactions = f.wait(3, InteractionState::Pending).await;
    assert!(
        f.app
            .interactions()
            .list(&f.session, Some(InteractionState::Expired), 200, None)
            .await
            .unwrap()
            .interactions
            .is_empty()
    );
    for interaction in interactions {
        f.app
            .interactions()
            .respond(&f.app, &f.session, &interaction.interaction_id, "allow")
            .await
            .unwrap();
        f.response().await;
    }
    f.stop().await;
}

#[tokio::test]
async fn confirmed_session_exit_rejects_a_response_even_before_observer_cleanup() {
    let f = Fixture::new().await;
    f.send(request("item/commandExecution/requestApproval", json!(1)));
    let interaction = f.wait(1, InteractionState::Pending).await.remove(0);
    f.app
        .session_commands()
        .request_exit(&f.session, None)
        .await
        .unwrap();
    let result = f
        .app
        .interactions()
        .respond(&f.app, &f.session, &interaction.interaction_id, "allow")
        .await;
    assert!(matches!(
        result,
        Err(pontia_application::interactions::InteractionError::Expired)
    ));
    assert!(f.replies.is_empty());
    assert_eq!(
        f.app
            .interactions()
            .get(&f.session, &interaction.interaction_id)
            .await
            .unwrap()
            .state,
        InteractionState::Expired
    );
    f.stop().await;
}
