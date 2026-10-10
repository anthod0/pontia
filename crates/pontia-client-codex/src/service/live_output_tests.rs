use crate::{CodexObserver, runtime::CodexRuntime};
use futures_util::{SinkExt, StreamExt};
use pontia_application::{
    AgentBindingService, AppState, CreateSessionRequest, LiveOutputItem, LiveOutputSnapshot,
    LiveOutputStreamEvent, UpsertAgentBindingRequest, clients::ClientRegistry,
};
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio::{
    net::UnixListener,
    sync::{Mutex, mpsc, watch},
    task::{JoinHandle, JoinSet},
};
use tokio_tungstenite::{accept_async, tungstenite::Message};

enum Command {
    Notify(Value),
    Disconnect,
}

#[derive(Default)]
struct ServerState {
    turns: Mutex<Vec<Value>>,
    connections: Mutex<Vec<mpsc::UnboundedSender<Command>>>,
    before_snapshot: Mutex<Vec<Value>>,
}

struct Fixture {
    app: AppState,
    session: String,
    root: tempfile::TempDir,
    socket: PathBuf,
    server_state: Arc<ServerState>,
    server: JoinHandle<()>,
    observer: JoinHandle<()>,
    shutdown: watch::Sender<bool>,
}

fn turn(items: Value) -> Value {
    json!({"id":"native-turn", "status":"inProgress", "itemsView":"full", "items":items})
}

fn delta(thread: &str, native: &str, item: &str, text: Value) -> Value {
    json!({"method":"item/agentMessage/delta","params":{"threadId":thread,"turnId":native,"itemId":item,"delta":text}})
}

impl Fixture {
    async fn new(initial: Value, queued: Vec<Value>) -> Self {
        let root = tempfile::tempdir().unwrap();
        let socket = root.path().join("app-server.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let server_state = Arc::new(ServerState::default());
        server_state.turns.lock().await.push(initial);
        *server_state.before_snapshot.lock().await = queued;
        let shared = server_state.clone();
        let codex_home = root.path().canonicalize().unwrap();
        let server = tokio::spawn(async move {
            let mut peers = JoinSet::new();
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                let shared = shared.clone();
                let codex_home = codex_home.clone();
                peers.spawn(async move {
                    let mut wire = accept_async(stream).await.unwrap();
                    let (sender, mut commands) = mpsc::unbounded_channel();
                    shared.connections.lock().await.push(sender);
                    loop {
                        tokio::select! {
                            command = commands.recv() => match command {
                                Some(Command::Notify(value)) => {
                                    if wire.send(Message::Text(value.to_string().into())).await.is_err() { break; }
                                }
                                _ => break,
                            },
                            frame = wire.next() => {
                                let Some(Ok(Message::Text(frame))) = frame else { break };
                                let request: Value = serde_json::from_str(&frame).unwrap();
                                let Some(id) = request.get("id") else { continue };
                                let thread = json!({"id":"thread", "cwd":codex_home,"canAcceptDirectInput":true,"status":{"type":"active"}});
                                let result = match request["method"].as_str().unwrap() {
                                    "initialize" => json!({"userAgent":"codex/dev","codexHome":codex_home,"platformFamily":"unix","platformOs":std::env::consts::OS}),
                                    "thread/resume" | "thread/read" => json!({"thread":thread,"model":"test-model"}),
                                    "thread/unsubscribe" => json!({"status":"unsubscribed"}),
                                    "thread/turns/list" => {
                                        let result = json!({"data":shared.turns.lock().await.clone(),"nextCursor":null});
                                        for notification in shared.before_snapshot.lock().await.drain(..) {
                                            wire.send(Message::Text(notification.to_string().into())).await.unwrap();
                                        }
                                        result
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
        CodexRuntime::install_for_test(root.path(), socket.clone())
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
        let (shutdown, receiver) = watch::channel(false);
        let observer = CodexObserver::new(&app, root.path().into());
        observer.prepare().await.unwrap();
        let observer = tokio::spawn(observer.run(receiver));
        let fixture = Self {
            app,
            session,
            root,
            socket,
            server_state,
            server,
            observer,
            shutdown,
        };
        fixture.wait_snapshot(|_| true).await;
        fixture
    }

    async fn notify(&self, value: Value) {
        self.server_state
            .connections
            .lock()
            .await
            .last()
            .unwrap()
            .send(Command::Notify(value))
            .unwrap();
    }

    async fn turn_id(&self) -> String {
        sqlx::query_scalar("SELECT turn_id FROM native_turn_bindings WHERE session_id=? AND client_turn_id='native-turn'")
            .bind(&self.session).fetch_one(&self.app.db()).await.unwrap()
    }

    async fn wait_snapshot(
        &self,
        matches: impl Fn(&LiveOutputSnapshot) -> bool,
    ) -> LiveOutputSnapshot {
        tokio::time::timeout(Duration::from_secs(6), async {
            loop {
                if let Some(snapshot) = self
                    .app
                    .live_output()
                    .subscribe_session(&self.session)
                    .initial_snapshot
                    && matches(&snapshot)
                {
                    return snapshot;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap_or_else(|error| {
            panic!(
                "{error}: {:?}",
                self.app
                    .live_output()
                    .subscribe_session(&self.session)
                    .initial_snapshot
            )
        })
    }

    async fn shutdown(self) {
        self.shutdown.send(true).unwrap();
        self.observer.await.unwrap();
        self.app.inbox_commands().stop_scheduling().await;
        CodexRuntime::shutdown(self.root.path()).await;
        self.server.abort();
        let _ = self.server.await;
    }
}

fn text(item: &str, value: &str) -> LiveOutputItem {
    LiveOutputItem::AssistantText {
        item_id: item.into(),
        text: value.into(),
    }
}

#[tokio::test]
async fn streams_external_turn_items_in_order_without_runtime_or_persisted_deltas() {
    let fixture = Fixture::new(turn(json!([])), Vec::new()).await;
    let initial = fixture.wait_snapshot(|_| true).await;
    let before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events")
        .fetch_one(&fixture.app.db())
        .await
        .unwrap();
    for (item, value) in [
        ("one", "Hello"),
        ("two", "other"),
        ("one", " world"),
        ("one", " world"),
    ] {
        fixture
            .notify(delta("thread", "native-turn", item, json!(value)))
            .await;
    }
    let snapshot = fixture
        .wait_snapshot(|snapshot| snapshot.sequence == initial.sequence + 4)
        .await;
    assert_eq!(
        snapshot.items,
        vec![text("one", "Hello world world"), text("two", "other")]
    );
    assert_eq!(snapshot.identity, initial.identity);
    let after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events")
        .fetch_one(&fixture.app.db())
        .await
        .unwrap();
    assert_eq!(after, before);
    let runtimes: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM session_runtimes")
        .fetch_one(&fixture.app.db())
        .await
        .unwrap();
    assert_eq!(runtimes, 0);
    let state: String = sqlx::query_scalar("SELECT state FROM turns WHERE turn_id=?")
        .bind(fixture.turn_id().await)
        .fetch_one(&fixture.app.db())
        .await
        .unwrap();
    assert_eq!(state, "running");
    fixture.shutdown().await;
}

#[tokio::test]
async fn rejects_unknown_native_identities_and_malformed_deltas_without_polluting_the_stream() {
    let fixture = Fixture::new(turn(json!([])), Vec::new()).await;
    let initial = fixture.wait_snapshot(|_| true).await;
    for notification in [
        delta("unbound", "native-turn", "item", json!("bad")),
        delta("thread", "unmapped-turn", "item", json!("bad")),
        delta("thread", "native-turn", "", json!("bad")),
        delta("thread", "native-turn", "item", json!(42)),
        delta("thread", "native-turn", "item", json!("")),
        json!({"method":"item/agentMessage/delta", "params":{"threadId":"thread","itemId":"item","delta":"bad"}}),
    ] {
        fixture.notify(notification).await;
    }
    fixture
        .notify(delta("thread", "native-turn", "item", json!("valid")))
        .await;
    let snapshot = fixture
        .wait_snapshot(|snapshot| snapshot.sequence > initial.sequence)
        .await;
    assert_eq!(snapshot.sequence, initial.sequence + 1);
    assert_eq!(snapshot.items, vec![text("item", "valid")]);
    fixture.shutdown().await;
}

#[tokio::test]
async fn native_snapshot_covers_queued_item_deltas_while_other_items_keep_streaming() {
    let fixture = Fixture::new(
        turn(json!([{"type":"agentMessage","id":"finished","text":"hello"}])),
        vec![
            delta("thread", "native-turn", "finished", json!("hello")),
            delta("thread", "native-turn", "streaming", json!("new")),
        ],
    )
    .await;
    let snapshot = fixture
        .wait_snapshot(|snapshot| snapshot.items.len() == 2)
        .await;
    assert_eq!(
        snapshot.items,
        vec![text("finished", "hello"), text("streaming", "new")]
    );
    fixture
        .notify(delta("thread", "native-turn", "streaming", json!(" text")))
        .await;
    let updated = fixture
        .wait_snapshot(|updated| updated.sequence > snapshot.sequence)
        .await;
    assert_eq!(
        updated.items,
        vec![text("finished", "hello"), text("streaming", "new text")]
    );
    fixture.shutdown().await;
}

#[tokio::test]
async fn completion_closes_the_shared_stream_and_ignores_late_deltas() {
    let fixture = Fixture::new(turn(json!([])), Vec::new()).await;
    fixture
        .notify(delta("thread", "native-turn", "item", json!("hello")))
        .await;
    fixture
        .wait_snapshot(|snapshot| !snapshot.items.is_empty())
        .await;
    let mut subscriber = fixture
        .app
        .live_output()
        .subscribe_session(&fixture.session);
    let mut completed =
        turn(json!([{"type":"agentMessage","id":"item","text":"hello","phase":"final_answer"}]));
    completed["status"] = json!("completed");
    *fixture.server_state.turns.lock().await = vec![completed.clone()];
    fixture
        .notify(json!({"method":"turn/completed","params":{"threadId":"thread","turn":completed}}))
        .await;
    let closed = tokio::time::timeout(Duration::from_secs(3), subscriber.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(closed, LiveOutputStreamEvent::Closed { .. }));
    fixture
        .notify(delta("thread", "native-turn", "item", json!("late")))
        .await;
    assert!(
        tokio::time::timeout(Duration::from_millis(100), subscriber.recv())
            .await
            .is_err()
    );
    assert!(
        fixture
            .app
            .live_output()
            .snapshot(&fixture.session, &fixture.turn_id().await)
            .is_none()
    );
    let state: String = sqlx::query_scalar("SELECT state FROM turns WHERE turn_id=?")
        .bind(fixture.turn_id().await)
        .fetch_one(&fixture.app.db())
        .await
        .unwrap();
    assert_eq!(state, "completed");
    fixture.shutdown().await;
}

#[tokio::test]
async fn replacement_connection_recovers_the_same_turn_and_rejects_the_old_connection() {
    let fixture = Fixture::new(turn(json!([])), Vec::new()).await;
    fixture
        .notify(delta("thread", "native-turn", "item", json!("before")))
        .await;
    let old_snapshot = fixture
        .wait_snapshot(|snapshot| !snapshot.items.is_empty())
        .await;
    let old_runtime = CodexRuntime::existing(fixture.root.path()).await.unwrap();
    let old_connection = old_runtime.connection().await.unwrap();
    *fixture.server_state.turns.lock().await = vec![turn(
        json!([{"type":"agentMessage","id":"item","text":"restored"}]),
    )];
    CodexRuntime::install_for_test(fixture.root.path(), fixture.socket.clone())
        .await
        .unwrap();
    assert!(old_runtime.connection().await.is_err());
    let _ = old_connection
        .events
        .send(crate::runtime::protocol::Notification {
            value: delta("thread", "native-turn", "item", json!("stale")),
            sequence: u64::MAX,
        });
    let restored = fixture
        .wait_snapshot(|snapshot| snapshot.items == vec![text("item", "restored")])
        .await;
    assert_eq!(restored.identity, old_snapshot.identity);
    fixture
        .notify(delta("thread", "native-turn", "item", json!(" after")))
        .await;
    let current = fixture
        .wait_snapshot(|snapshot| snapshot.items == vec![text("item", "restored after")])
        .await;
    assert_eq!(current.identity, old_snapshot.identity);
    let state: String = sqlx::query_scalar("SELECT state FROM turns WHERE turn_id=?")
        .bind(fixture.turn_id().await)
        .fetch_one(&fixture.app.db())
        .await
        .unwrap();
    assert_eq!(state, "running");
    fixture.shutdown().await;
}

#[tokio::test]
async fn a_lost_volatile_snapshot_is_rebuilt_before_the_next_update() {
    let fixture = Fixture::new(turn(json!([])), Vec::new()).await;
    let initial = fixture.wait_snapshot(|_| true).await;
    *fixture.server_state.turns.lock().await = vec![turn(
        json!([{"type":"agentMessage","id":"item","text":"recovered"}]),
    )];
    fixture
        .app
        .live_output()
        .discard_turn(&fixture.session, &fixture.turn_id().await);
    fixture
        .notify(delta("thread", "native-turn", "next", json!("continued")))
        .await;
    let snapshot = fixture
        .wait_snapshot(|snapshot| snapshot.items.len() == 2)
        .await;
    assert_eq!(snapshot.identity, initial.identity);
    assert_eq!(
        snapshot.items,
        vec![text("item", "recovered"), text("next", "continued")]
    );
    fixture.shutdown().await;
}

#[tokio::test]
async fn disconnect_invalidates_display_without_ending_the_turn() {
    let fixture = Fixture::new(turn(json!([])), Vec::new()).await;
    fixture
        .notify(delta("thread", "native-turn", "item", json!("hello")))
        .await;
    fixture
        .wait_snapshot(|snapshot| !snapshot.items.is_empty())
        .await;
    let mut subscriber = fixture
        .app
        .live_output()
        .subscribe_session(&fixture.session);
    fixture
        .server_state
        .connections
        .lock()
        .await
        .last()
        .unwrap()
        .send(Command::Disconnect)
        .unwrap();
    assert!(matches!(
        tokio::time::timeout(Duration::from_secs(3), subscriber.recv())
            .await
            .unwrap()
            .unwrap(),
        LiveOutputStreamEvent::Closed { .. }
    ));
    let state: String = sqlx::query_scalar("SELECT state FROM turns WHERE turn_id=?")
        .bind(fixture.turn_id().await)
        .fetch_one(&fixture.app.db())
        .await
        .unwrap();
    assert_eq!(state, "running");
    fixture.shutdown().await;
}

#[tokio::test]
async fn exited_sessions_do_not_receive_deltas_or_restore_after_connection_replacement() {
    let fixture = Fixture::new(turn(json!([])), Vec::new()).await;
    fixture
        .app
        .session_commands()
        .terminate_session(&fixture.session)
        .await
        .unwrap();
    fixture
        .notify(delta("thread", "native-turn", "item", json!("late")))
        .await;
    CodexRuntime::install_for_test(fixture.root.path(), fixture.socket.clone())
        .await
        .unwrap();
    let mut subscriber = fixture
        .app
        .live_output()
        .subscribe_session(&fixture.session);
    assert!(
        tokio::time::timeout(Duration::from_millis(1200), subscriber.recv())
            .await
            .is_err()
    );
    assert!(subscriber.initial_snapshot.is_none());
    let session: String = sqlx::query_scalar("SELECT state FROM sessions WHERE session_id=?")
        .bind(&fixture.session)
        .fetch_one(&fixture.app.db())
        .await
        .unwrap();
    assert_eq!(session, "exited");
    fixture.shutdown().await;
}

#[tokio::test]
async fn observer_lag_resubscribes_and_recovers_without_a_terminal_fact() {
    let fixture = Fixture::new(turn(json!([])), Vec::new()).await;
    let initial = fixture.wait_snapshot(|_| true).await;
    let mut subscriber = fixture
        .app
        .live_output()
        .subscribe_session(&fixture.session);
    *fixture.server_state.turns.lock().await = vec![turn(
        json!([{"type":"agentMessage","id":"item","text":"recovered"}]),
    )];
    *fixture.server_state.before_snapshot.lock().await = (0..4200)
        .map(|_| delta("thread", "native-turn", "item", json!("old")))
        .collect();
    fixture.notify(json!({"method":"turn/started","params":{"threadId":"thread","turn":{"id":"native-turn"}}})).await;
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if matches!(
                subscriber.recv().await.unwrap(),
                LiveOutputStreamEvent::Closed { .. }
            ) {
                break;
            }
        }
    })
    .await
    .unwrap();
    let restored = fixture
        .wait_snapshot(|snapshot| snapshot.items == vec![text("item", "recovered")])
        .await;
    assert_eq!(restored.identity, initial.identity);
    let state: String = sqlx::query_scalar("SELECT state FROM turns WHERE turn_id=?")
        .bind(fixture.turn_id().await)
        .fetch_one(&fixture.app.db())
        .await
        .unwrap();
    assert_eq!(state, "running");
    fixture.shutdown().await;
}

#[tokio::test]
async fn unsafe_snapshot_identities_fail_closed_without_blocking_native_completion() {
    let mut summary = turn(json!([{"type":"agentMessage","id":"msg-one","text":"hello"}]));
    summary["itemsView"] = json!("summary");
    let mut missing_items = turn(json!([]));
    missing_items.as_object_mut().unwrap().remove("items");
    for snapshot in [
        turn(json!([{"type":"agentMessage","id":"item-1","text":"hello"}])),
        summary,
        missing_items,
    ] {
        let fixture = Fixture::new(turn(json!([])), Vec::new()).await;
        let mut subscriber = fixture
            .app
            .live_output()
            .subscribe_session(&fixture.session);
        *fixture.server_state.turns.lock().await = vec![snapshot];
        *fixture.server_state.before_snapshot.lock().await =
            vec![delta("thread", "native-turn", "msg-one", json!("hello"))];
        let runtime = CodexRuntime::install_for_test(fixture.root.path(), fixture.socket.clone())
            .await
            .unwrap();
        assert!(matches!(
            tokio::time::timeout(Duration::from_secs(3), subscriber.recv())
                .await
                .unwrap()
                .unwrap(),
            LiveOutputStreamEvent::Closed { .. }
        ));
        tokio::time::timeout(Duration::from_secs(6), async {
            while runtime.subscription(&fixture.session).await
                != Some(crate::runtime::SubscriptionState::Available)
            {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        fixture
            .notify(delta("thread", "native-turn", "msg-one", json!("late")))
            .await;
        assert!(
            tokio::time::timeout(Duration::from_millis(100), subscriber.recv())
                .await
                .is_err()
        );
        assert!(
            fixture
                .app
                .live_output()
                .snapshot(&fixture.session, &fixture.turn_id().await)
                .is_none()
        );
        let mut completed = turn(
            json!([{"type":"agentMessage","id":"msg-one","text":"hello late","phase":"final_answer"}]),
        );
        completed["status"] = json!("completed");
        *fixture.server_state.turns.lock().await = vec![completed.clone()];
        fixture
            .notify(
                json!({"method":"turn/completed","params":{"threadId":"thread","turn":completed}}),
            )
            .await;
        let id = fixture.turn_id().await;
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                let state: String = sqlx::query_scalar("SELECT state FROM turns WHERE turn_id=?")
                    .bind(&id)
                    .fetch_one(&fixture.app.db())
                    .await
                    .unwrap();
                if state == "completed" {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        fixture.shutdown().await;
    }
}
