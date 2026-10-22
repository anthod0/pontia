use crate::{CodexObserver, runtime::CodexRuntime};
use futures_util::{SinkExt, StreamExt};
use pontia_application::{
    AgentBindingService, AppState, CreateSessionRequest, UpsertAgentBindingRequest,
    clients::ClientRegistry,
};
use serde_json::{Value, json};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::{
    net::UnixListener,
    sync::{Mutex, mpsc, watch},
    task::{JoinHandle, JoinSet},
};
use tokio_tungstenite::{accept_async, tungstenite::Message};

#[derive(Default)]
struct Server {
    turns: Mutex<HashMap<String, Vec<Value>>>,
    requests: Mutex<Vec<Value>>,
    peers: Mutex<Vec<mpsc::UnboundedSender<Option<Value>>>>,
    rejection: Mutex<bool>,
    response: Mutex<Option<Value>>,
    before_snapshot: Mutex<Vec<Value>>,
}
struct Fixture {
    root: tempfile::TempDir,
    app: AppState,
    sessions: Vec<String>,
    shared: Arc<Server>,
    server: JoinHandle<()>,
    observer: JoinHandle<()>,
    shutdown: watch::Sender<bool>,
}
fn native_turn(id: &str, status: &str, items: Value) -> Value {
    json!({"id":id,"status":status,"items":items,"itemsView":"full"})
}
fn item_event(thread: &str, turn: &str, id: &str, completed: bool) -> Value {
    json!({"method":if completed {"item/completed"} else {"item/started"},"params":{"threadId":thread,"turnId":turn,"item":{"type":"contextCompaction","id":id}}})
}
impl Fixture {
    async fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let socket = root.path().join("app-server.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let shared = Arc::new(Server::default());
        let server_shared = shared.clone();
        let cwd = root.path().canonicalize().unwrap();
        let server = tokio::spawn(async move {
            let mut peers = JoinSet::new();
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                let shared = server_shared.clone();
                let cwd = cwd.clone();
                peers.spawn(async move {
                    let mut wire = accept_async(stream).await.unwrap();
                    let (sender, mut commands) = mpsc::unbounded_channel();
                    shared.peers.lock().await.push(sender);
                    loop {
                        tokio::select! {
                            command = commands.recv() => match command {
                                Some(Some(event)) => { if wire.send(Message::Text(event.to_string().into())).await.is_err() { break; } }
                                _ => break,
                            },
                            frame = wire.next() => {
                                let Some(Ok(Message::Text(frame))) = frame else { break };
                                let request: Value = serde_json::from_str(&frame).unwrap();
                                let Some(id) = request.get("id").cloned() else { continue };
                                shared.requests.lock().await.push(request.clone());
                                let thread = request["params"]["threadId"].as_str().unwrap_or("thread-0");
                                let result = match request["method"].as_str().unwrap() {
                                    "initialize" => json!({"userAgent":"codex/test","codexHome":cwd,"platformFamily":"unix","platformOs":std::env::consts::OS}),
                                    "thread/resume" | "thread/read" => json!({"thread":{"id":thread,"cwd":cwd,"canAcceptDirectInput":true,"status":{"type":"idle"}},"model":"test-model"}),
                                    "thread/turns/list" => {
                                        let result = json!({"data":shared.turns.lock().await.get(thread).cloned().unwrap_or_default(),"nextCursor":null});
                                        for event in shared.before_snapshot.lock().await.drain(..) {
                                            wire.send(Message::Text(event.to_string().into())).await.unwrap();
                                        }
                                        result
                                    },
                                    "thread/compact/start" => {
                                        if *shared.rejection.lock().await {
                                            wire.send(Message::Text(json!({"id":id,"error":{"code":-32600,"message":"rejected"}}).to_string().into())).await.unwrap();
                                            continue;
                                        }
                                        let turn = format!("compact-{thread}");
                                        shared.turns.lock().await.entry(thread.into()).or_default().push(native_turn(&turn,"inProgress",json!([])));
                                        // These arrive before the RPC acknowledgement.
                                        wire.send(Message::Text(json!({"method":"turn/started","params":{"threadId":thread,"turn":native_turn(&turn,"inProgress",json!([]))}}).to_string().into())).await.unwrap();
                                        wire.send(Message::Text(item_event(thread,&turn,"live-id",false).to_string().into())).await.unwrap();
                                        shared.response.lock().await.clone().unwrap_or_else(|| json!({}))
                                    }
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
        let pool = pontia_storage_sqlite::connect_sqlite("sqlite::memory:")
            .await
            .unwrap();
        pontia_storage_sqlite::run_migrations(&pool).await.unwrap();
        pontia_storage_sqlite::repositories::agent_clients::SqliteAgentClientRepository::new(
            pool.clone(),
        )
        .save("codex", "default", Some(true), None)
        .await
        .unwrap();
        let mut clients = ClientRegistry::default();
        clients.register(crate::registration());
        let app = AppState::builder(pool, root.path().into())
            .clients(clients)
            .build();
        let mut sessions = Vec::new();
        for index in 0..2 {
            let session = app
                .session_commands()
                .create_session(
                    serde_json::from_value::<CreateSessionRequest>(
                        json!({"client_type":"codex","workspace":root.path()}),
                    )
                    .unwrap(),
                )
                .await
                .unwrap()
                .session_id()
                .unwrap()
                .to_owned();
            let thread = format!("thread-{index}");
            shared.turns.lock().await.insert(thread.clone(), vec![native_turn("user-turn","completed",json!([{"type":"userMessage","id":"user","content":[{"type":"text","text":"hello"}]}]))]);
            AgentBindingService::new(app.db())
                .upsert_binding(UpsertAgentBindingRequest {
                    session_id: session.clone(),
                    client_type: "codex".into(),
                    launch_cwd: root.path().display().to_string(),
                    client_session_key: thread,
                    client_session_file: None,
                    metadata: json!({}),
                })
                .await
                .unwrap();
            sessions.push(session);
        }
        let (shutdown, receiver) = watch::channel(false);
        let observer = CodexObserver::new(&app, root.path().into());
        observer.prepare().await.unwrap();
        let observer = tokio::spawn(observer.run(receiver));
        let fixture = Self {
            root,
            app,
            sessions,
            shared,
            server,
            observer,
            shutdown,
        };
        fixture.wait_ready().await;
        fixture
    }
    async fn wait_ready(&self) {
        tokio::time::timeout(Duration::from_secs(8), async {
            loop {
                let mut ready = true;
                for session in &self.sessions {
                    let session = self
                        .app
                        .queries()
                        .get_session(session)
                        .await
                        .unwrap()
                        .unwrap();
                    ready &= session.state == "idle" && session.capabilities.manual_compaction;
                }
                if ready {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
    }
    async fn notify(&self, value: Value) {
        self.shared
            .peers
            .lock()
            .await
            .last()
            .unwrap()
            .send(Some(value))
            .unwrap();
    }
    async fn facts(&self, index: usize, count: usize) -> Vec<pontia_application::views::EventView> {
        tokio::time::timeout(Duration::from_secs(8), async {
            loop {
                let facts: Vec<_> = self
                    .app
                    .queries()
                    .list_session_events(&self.sessions[index])
                    .await
                    .unwrap()
                    .into_iter()
                    .filter(|event| event.event_type.starts_with("session.compaction_"))
                    .collect();
                if facts.len() >= count {
                    return facts;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap()
    }
    async fn finish(&self, thread: &str, status: &str, error: Value) {
        let turn = format!("compact-{thread}");
        let items = if status == "completed" {
            json!([{"type":"contextCompaction","id":"item-1"}])
        } else {
            json!([])
        };
        let mut snapshot = native_turn(&turn, status, items);
        snapshot["error"] = error;
        *self
            .shared
            .turns
            .lock()
            .await
            .get_mut(thread)
            .unwrap()
            .last_mut()
            .unwrap() = snapshot.clone();
        if status == "completed" {
            self.notify(item_event(thread, &turn, "live-id", true))
                .await;
        }
        self.notify(
            json!({"method":"turn/completed","params":{"threadId":thread,"turn":snapshot}}),
        )
        .await;
    }
    async fn reconnect(&self) {
        self.shared
            .peers
            .lock()
            .await
            .last()
            .unwrap()
            .send(None)
            .unwrap();
        tokio::time::timeout(Duration::from_secs(8), async {
            loop {
                if self.shared.peers.lock().await.len() > 1 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        self.wait_ready().await;
    }
    async fn barrier(&self) {
        self.notify(json!({"method":"thread/settings/updated","params":{"threadId":"thread-0","threadSettings":{"model":"barrier-model"}}})).await;
        tokio::time::timeout(Duration::from_secs(8), async {
            loop {
                if self
                    .app
                    .queries()
                    .list_session_events(&self.sessions[0])
                    .await
                    .unwrap()
                    .iter()
                    .any(|event| event.payload["model"] == "barrier-model")
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
    }
    async fn close(self) {
        self.shutdown.send(true).unwrap();
        self.observer.await.unwrap();
        self.app.inbox_commands().stop_scheduling().await;
        CodexRuntime::shutdown(self.root.path()).await;
        self.server.abort();
    }
}

#[tokio::test]
async fn manual_compaction_routes_sessions_correlates_and_survives_reconnect_without_turns() {
    let fixture = Fixture::new().await;
    let first = fixture
        .app
        .session_commands()
        .compact_session(&fixture.sessions[0])
        .await
        .unwrap();
    let second = fixture
        .app
        .session_commands()
        .compact_session(&fixture.sessions[1])
        .await
        .unwrap();
    fixture.facts(1, 1).await;
    let started = fixture.facts(0, 1).await;
    assert_eq!(started[0].payload["operation_id"], first.operation_id);
    assert_eq!(started[0].payload["trigger"], "manual");
    fixture
        .notify(item_event("thread-0", "compact-thread-0", "live-id", false))
        .await;
    fixture.finish("thread-0", "completed", Value::Null).await;
    fixture.finish("thread-1", "interrupted", Value::Null).await;
    fixture.facts(0, 2).await;
    let failed = fixture.facts(1, 2).await;
    assert_eq!(failed[1].payload["operation_id"], second.operation_id);
    assert_eq!(failed[1].payload["aborted"], true);
    assert!(failed[1].payload["error_message"].is_null());
    fixture.reconnect().await;
    fixture
        .notify(item_event("thread-0", "compact-thread-0", "live-id", true))
        .await;
    fixture.barrier().await;
    let facts = fixture.facts(0, 2).await;
    assert_eq!(facts.len(), 2);
    assert_eq!(facts[1].payload["tokens_before"], Value::Null);
    assert_eq!(facts[1].payload["tokens_after"], Value::Null);
    assert!(facts.iter().all(|event| event.turn_id.is_none()));
    let requests = fixture.shared.requests.lock().await;
    let compact: Vec<_> = requests
        .iter()
        .filter(|request| request["method"] == "thread/compact/start")
        .collect();
    assert_eq!(compact.len(), 2);
    assert_eq!(compact[0]["params"], json!({"threadId":"thread-0"}));
    assert_eq!(compact[1]["params"], json!({"threadId":"thread-1"}));
    drop(requests);
    for session in &fixture.sessions {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM turns WHERE session_id=?")
            .bind(session)
            .fetch_one(&fixture.app.db())
            .await
            .unwrap();
        assert_eq!(count, 1);
        assert_eq!(
            fixture
                .app
                .queries()
                .get_session(session)
                .await
                .unwrap()
                .unwrap()
                .state,
            "idle"
        );
    }
    fixture.close().await;
}

#[tokio::test]
async fn native_failure_is_reported_with_real_error_and_automatic_trigger_is_unknown() {
    let fixture = Fixture::new().await;
    fixture
        .notify(item_event("thread-1", "user-turn", "auto-one", false))
        .await;
    fixture.notify(json!({"method":"error","params":{"threadId":"thread-1","turnId":"user-turn","willRetry":true,"error":{"message":"retrying"}}})).await;
    fixture.notify(json!({"method":"error","params":{"threadId":"thread-1","turnId":"user-turn","willRetry":false,"error":{"message":"native failure"}}})).await;
    let facts = fixture.facts(1, 2).await;
    assert_eq!(facts.len(), 2);
    assert_eq!(facts[0].payload["trigger"], "unknown");
    assert_eq!(
        facts[0].payload["operation_id"],
        facts[1].payload["operation_id"]
    );
    assert_eq!(facts[1].payload["aborted"], false);
    assert_eq!(facts[1].payload["error_message"], "native failure");
    assert!(fixture.facts(0, 0).await.is_empty());
    fixture.close().await;
}

#[tokio::test]
async fn rejected_compaction_can_retry_and_unavailable_control_preserves_observation_capabilities()
{
    let fixture = Fixture::new().await;
    *fixture.shared.rejection.lock().await = true;
    assert!(matches!(
        fixture
            .app
            .session_commands()
            .compact_session(&fixture.sessions[0])
            .await,
        Err(pontia_core::Error::StateConflict(_))
    ));
    assert!(fixture.facts(0, 0).await.is_empty());
    *fixture.shared.rejection.lock().await = false;
    fixture
        .app
        .session_commands()
        .compact_session(&fixture.sessions[0])
        .await
        .unwrap();
    fixture.facts(0, 1).await;
    fixture
        .finish("thread-0", "failed", json!({"message":"failed"}))
        .await;
    let facts = fixture.facts(0, 2).await;
    assert_eq!(facts[1].payload["aborted"], false);
    fixture.shutdown.send(true).unwrap();
    fixture.observer.await.unwrap();
    CodexRuntime::shutdown(fixture.root.path()).await;
    let session = fixture
        .app
        .queries()
        .get_session(&fixture.sessions[0])
        .await
        .unwrap()
        .unwrap();
    assert!(!session.capabilities.manual_compaction);
    assert!(session.capabilities.compaction_events);
    assert!(session.capabilities.compaction_history);
    fixture.app.inbox_commands().stop_scheduling().await;
    fixture.server.abort();
}

#[tokio::test]
async fn reconnect_recovers_manual_completion_for_an_interrupted_session_with_a_lost_final_notification()
 {
    let fixture = Fixture::new().await;
    sqlx::query("UPDATE sessions SET state='interrupted' WHERE session_id=?")
        .bind(&fixture.sessions[0])
        .execute(&fixture.app.db())
        .await
        .unwrap();
    let accepted = fixture
        .app
        .session_commands()
        .compact_session(&fixture.sessions[0])
        .await
        .unwrap();
    fixture.facts(0, 1).await;
    *fixture
        .shared
        .turns
        .lock()
        .await
        .get_mut("thread-0")
        .unwrap()
        .last_mut()
        .unwrap() = native_turn(
        "compact-thread-0",
        "completed",
        json!([{"type":"contextCompaction","id":"item-1"}]),
    );
    fixture.reconnect().await;
    let facts = fixture.facts(0, 2).await;
    assert_eq!(facts.len(), 2);
    assert_eq!(facts[1].payload["operation_id"], accepted.operation_id);
    assert_eq!(facts[1].payload["trigger"], "manual");
    assert_eq!(facts[1].event_type, "session.compaction_completed");
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM turns WHERE session_id=?")
        .bind(&fixture.sessions[0])
        .fetch_one(&fixture.app.db())
        .await
        .unwrap();
    assert_eq!(count, 1);
    fixture.close().await;
}

#[tokio::test]
async fn later_automatic_compactions_do_not_reuse_an_earlier_snapshot_or_failed_operation() {
    let fixture = Fixture::new().await;
    fixture
        .notify(item_event("thread-1", "user-turn", "failed-auto", false))
        .await;
    fixture.notify(json!({"method":"error","params":{"threadId":"thread-1","turnId":"user-turn","willRetry":false,"error":null}})).await;
    fixture.facts(1, 2).await;
    fixture
        .notify(item_event(
            "thread-1",
            "user-turn",
            "successful-auto",
            false,
        ))
        .await;
    fixture
        .notify(item_event("thread-1", "user-turn", "successful-auto", true))
        .await;
    fixture.facts(1, 4).await;
    fixture
        .shared
        .turns
        .lock()
        .await
        .get_mut("thread-1")
        .unwrap()[0]["items"]
        .as_array_mut()
        .unwrap()
        .push(json!({"type":"contextCompaction","id":"item-1"}));
    fixture.reconnect().await;
    fixture.barrier().await;
    assert_eq!(fixture.facts(1, 4).await.len(), 4);
    fixture
        .notify(item_event("thread-1", "user-turn", "later-auto", false))
        .await;
    fixture
        .notify(item_event("thread-1", "user-turn", "later-auto", true))
        .await;
    let facts = fixture.facts(1, 6).await;
    assert_eq!(facts.len(), 6);
    assert_ne!(
        facts[0].payload["operation_id"],
        facts[2].payload["operation_id"]
    );
    assert_ne!(
        facts[2].payload["operation_id"],
        facts[4].payload["operation_id"]
    );
    assert_eq!(facts[5].payload["trigger"], "unknown");
    fixture.close().await;
}

#[tokio::test]
async fn invalid_acknowledgement_retains_manual_correlation_and_never_retries_the_native_request() {
    let fixture = Fixture::new().await;
    *fixture.shared.response.lock().await = Some(json!({"unexpected":true}));
    assert!(matches!(
        fixture
            .app
            .session_commands()
            .compact_session(&fixture.sessions[0])
            .await,
        Err(pontia_core::Error::ControlUnknown(_))
    ));
    let started = fixture.facts(0, 1).await;
    assert_eq!(started[0].payload["trigger"], "manual");
    fixture.finish("thread-0", "completed", Value::Null).await;
    let facts = fixture.facts(0, 2).await;
    assert_eq!(
        facts[0].payload["operation_id"],
        facts[1].payload["operation_id"]
    );
    assert_eq!(
        fixture
            .shared
            .requests
            .lock()
            .await
            .iter()
            .filter(|request| request["method"] == "thread/compact/start")
            .count(),
        1
    );
    fixture.close().await;
}

#[tokio::test]
async fn external_standalone_compaction_never_creates_or_leaves_an_active_pontia_turn() {
    let fixture = Fixture::new().await;
    let native = "external-compact";
    fixture
        .shared
        .turns
        .lock()
        .await
        .get_mut("thread-0")
        .unwrap()
        .push(native_turn(native, "inProgress", json!([])));
    fixture.notify(json!({"method":"turn/started","params":{"threadId":"thread-0","turn":native_turn(native,"inProgress",json!([]))}})).await;
    fixture
        .notify(item_event("thread-0", native, "external-id", false))
        .await;
    let started = fixture.facts(0, 1).await;
    assert_eq!(started[0].payload["trigger"], "unknown");
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM turns WHERE session_id=?")
        .bind(&fixture.sessions[0])
        .fetch_one(&fixture.app.db())
        .await
        .unwrap();
    assert_eq!(count, 1);
    assert_eq!(
        fixture
            .app
            .queries()
            .get_session(&fixture.sessions[0])
            .await
            .unwrap()
            .unwrap()
            .state,
        "idle"
    );
    fixture
        .notify(item_event("thread-0", native, "external-id", true))
        .await;
    *fixture
        .shared
        .turns
        .lock()
        .await
        .get_mut("thread-0")
        .unwrap()
        .last_mut()
        .unwrap() = native_turn(
        native,
        "completed",
        json!([{"type":"contextCompaction","id":"item-0"}]),
    );
    // Real turn/completed carries no compaction items in its summary.
    fixture.notify(json!({"method":"turn/completed","params":{"threadId":"thread-0","turn":native_turn(native,"completed",json!([]))}})).await;
    fixture.facts(0, 2).await;
    fixture.barrier().await;
    assert_eq!(
        fixture
            .app
            .queries()
            .get_session(&fixture.sessions[0])
            .await
            .unwrap()
            .unwrap()
            .state,
        "idle"
    );
    fixture.reconnect().await;
    assert_eq!(fixture.facts(0, 2).await.len(), 2);
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM turns WHERE session_id=?")
        .bind(&fixture.sessions[0])
        .fetch_one(&fixture.app.db())
        .await
        .unwrap();
    assert_eq!(count, 1);
    fixture.close().await;
}

#[tokio::test]
async fn unknown_offline_history_neither_synthesizes_operations_nor_swallows_new_or_replayed_notifications()
 {
    let fixture = Fixture::new().await;
    fixture
        .shared
        .turns
        .lock()
        .await
        .get_mut("thread-0")
        .unwrap()[0]["items"]
        .as_array_mut()
        .unwrap()
        .push(json!({"type":"contextCompaction","id":"item-1"}));
    fixture.shared.before_snapshot.lock().await.push(item_event(
        "thread-0",
        "user-turn",
        "new-during-snapshot",
        false,
    ));
    fixture.reconnect().await;
    let started = fixture.facts(0, 1).await;
    assert_eq!(started.len(), 1);
    fixture
        .notify(item_event(
            "thread-0",
            "user-turn",
            "new-during-snapshot",
            true,
        ))
        .await;
    fixture.facts(0, 2).await;
    // A late native replay has its own real item identity; no synthetic lifecycle
    // was generated for the old offline checkpoint.
    fixture
        .notify(item_event(
            "thread-0",
            "user-turn",
            "offline-real-id",
            false,
        ))
        .await;
    fixture
        .notify(item_event("thread-0", "user-turn", "offline-real-id", true))
        .await;
    let facts = fixture.facts(0, 4).await;
    assert_eq!(facts.len(), 4);
    assert_ne!(
        facts[0].payload["operation_id"],
        facts[2].payload["operation_id"]
    );
    fixture
        .notify(item_event(
            "thread-0",
            "user-turn",
            "offline-real-id",
            false,
        ))
        .await;
    fixture
        .notify(item_event("thread-0", "user-turn", "offline-real-id", true))
        .await;
    fixture.barrier().await;
    assert_eq!(fixture.facts(0, 4).await.len(), 4);
    fixture.close().await;
}

#[tokio::test]
async fn an_empty_native_start_can_later_establish_a_real_user_turn_after_pre_turn_compaction() {
    let fixture = Fixture::new().await;
    let native = "next-user-turn";
    fixture
        .shared
        .turns
        .lock()
        .await
        .get_mut("thread-0")
        .unwrap()
        .push(native_turn(native, "inProgress", json!([])));
    fixture.notify(json!({"method":"turn/started","params":{"threadId":"thread-0","turn":native_turn(native,"inProgress",json!([]))}})).await;
    fixture
        .notify(item_event("thread-0", native, "pre-turn-auto", false))
        .await;
    fixture
        .notify(item_event("thread-0", native, "pre-turn-auto", true))
        .await;
    fixture.facts(0, 2).await;
    fixture.notify(json!({"method":"item/started","params":{"threadId":"thread-0","turnId":native,"item":{"type":"userMessage","id":"next-user","content":[{"type":"text","text":"actual user execution"}]}}})).await;
    fixture.barrier().await;
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM turns WHERE session_id=?")
        .bind(&fixture.sessions[0])
        .fetch_one(&fixture.app.db())
        .await
        .unwrap();
    assert_eq!(count, 2);
    assert_eq!(
        fixture
            .app
            .queries()
            .get_session(&fixture.sessions[0])
            .await
            .unwrap()
            .unwrap()
            .state,
        "busy"
    );
    let final_turn = native_turn(
        native,
        "completed",
        json!([{"type":"agentMessage","id":"answer","phase":"final_answer","text":"done"}]),
    );
    *fixture
        .shared
        .turns
        .lock()
        .await
        .get_mut("thread-0")
        .unwrap()
        .last_mut()
        .unwrap() = final_turn.clone();
    fixture
        .notify(
            json!({"method":"turn/completed","params":{"threadId":"thread-0","turn":final_turn}}),
        )
        .await;
    fixture.notify(json!({"method":"thread/settings/updated","params":{"threadId":"thread-0","threadSettings":{"model":"after-user-completion"}}})).await;
    tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            if fixture
                .app
                .queries()
                .get_session(&fixture.sessions[0])
                .await
                .unwrap()
                .unwrap()
                .state
                == "idle"
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(fixture.facts(0, 2).await.len(), 2);
    fixture.close().await;
}
