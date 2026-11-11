use super::*;
use crate::service::confirmed_exits;
use crate::{CodexObserver, proxy::Proxy};
use futures_util::{SinkExt, StreamExt};
use pontia_application::clients::ClientRegistry;
use serde_json::{Value, json};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::{
    net::UnixListener,
    sync::{Mutex, watch},
    task::JoinSet,
};
use tokio_tungstenite::{accept_async, tungstenite::Message};

struct Thread {
    value: Value,
    materialized: bool,
    turns: Vec<Value>,
    subscribers: Vec<mpsc::UnboundedSender<Value>>,
}

impl Thread {
    fn notify(&self, event: Value) {
        for peer in &self.subscribers {
            let _ = peer.send(event.clone());
        }
    }
}

struct Fixture {
    app: AppState,
    root: tempfile::TempDir,
    threads: Arc<Mutex<HashMap<String, Thread>>>,
    faults: Arc<Mutex<std::collections::HashSet<String>>>,
    tasks: JoinSet<()>,
    shutdown: watch::Sender<bool>,
}

impl Fixture {
    async fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let socket = root.path().join("native.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let threads = Arc::new(Mutex::new(HashMap::<String, Thread>::new()));
        let shared = threads.clone();
        let faults = Arc::new(Mutex::new(std::collections::HashSet::<String>::new()));
        let shared_faults = faults.clone();
        let home = root.path().canonicalize().unwrap();
        let mut tasks = JoinSet::new();
        tasks.spawn(async move {
            let mut peers = JoinSet::new();
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                let threads = shared.clone();
                let faults = shared_faults.clone();
                let home = home.clone();
                peers.spawn(async move {
                    let mut wire = accept_async(stream).await.unwrap();
                    let (sender, mut events) = mpsc::unbounded_channel::<Value>();
                    let mut pontia = false;
                    loop {
                        let request = tokio::select! {
                            event = events.recv() => {
                                let Some(event) = event else { break };
                                if wire.send(Message::text(event.to_string())).await.is_err() { break; }
                                continue;
                            }
                            frame = wire.next() => {
                                let Some(Ok(Message::Text(frame))) = frame else { break };
                                serde_json::from_str::<Value>(&frame).unwrap()
                            }
                        };
                        let Some(id) = request.get("id") else { continue };
                        let method = request["method"].as_str().unwrap();
                        let params = &request["params"];
                        let native = params["threadId"].as_str().unwrap_or("");
                        let mut threads = threads.lock().await;
                        let response = if params["reject"] == true || faults.lock().await.contains(method) {
                            json!({"id":id,"error":{"code":-32600,"message":"rejected"}})
                        } else if method == "thread/read" && !threads[native].materialized {
                            json!({"id":id,"error":{"code":-32603,"message":"failed to read thread: rollout is empty"}})
                        } else if method == "thread/resume" && !threads[native].materialized {
                            json!({"id":id,"error":{"code":-32600,"message":format!("no rollout found for thread id {native}")}})
                        } else if method == "thread/turns/list" && !threads[native].materialized {
                            json!({"id":id,"error":{"code":-32600,"message":format!("thread {native} is not materialized yet; thread/turns/list is unavailable before first user message")}})
                        } else {
                            let result = match method {
                                "initialize" => {
                                    pontia = params["clientInfo"]["name"] == "pontia";
                                    json!({"userAgent":"codex/test","codexHome":home,"platformFamily":"unix","platformOs":std::env::consts::OS})
                                }
                                "config/read" => json!({"config":{}}),
                                "thread/start" | "thread/fork" => {
                                    let inherited = threads.get(native).map(|parent| {
                                        parent.turns.iter().take_while(|turn| params["beforeTurnId"].is_null() || turn["id"] != params["beforeTurnId"]).cloned().collect::<Vec<_>>()
                                    }).unwrap_or_default();
                                    let parent = params["threadId"].clone();
                                    let native = format!("thread-{}", threads.len());
                                    let value = json!({"id":native,"sessionId":"shared-native-root","forkedFromId":parent,"cwd":home,"ephemeral":params["ephemeral"] == true,"canAcceptDirectInput":true,"status":{"type":"idle"}});
                                    threads.insert(native, Thread {
                                        value: value.clone(), materialized: method == "thread/fork",
                                        turns: inherited, subscribers: vec![sender.clone()],
                                    });
                                    json!({"thread":value,"model":"cli-model"})
                                }
                                "thread/read" => json!({"thread":threads[native].value}),
                                "thread/resume" => {
                                    if pontia {
                                        assert!(params.get("config").is_none(), "proxy attachment must preserve CLI configuration");
                                    }
                                    let thread = threads.get_mut(native).unwrap();
                                    thread.value["status"]["type"] = json!("idle");
                                    thread.subscribers.push(sender.clone());
                                    json!({"thread":thread.value,"model":"cli-model"})
                                }
                                "thread/turns/list" => json!({"data":threads[native].turns,"nextCursor":null}),
                                "turn/start" => {
                                    let thread = threads.get_mut(native).unwrap();
                                    thread.materialized = true;
                                    let turn = json!({"id":format!("turn-{}", thread.turns.len()),"status":"inProgress","items":[{"id":"input","type":"userMessage","content":[{"type":"text","text":params.pointer("/input/0/text").and_then(Value::as_str).unwrap_or("from CLI")}]}]});
                                    thread.turns.push(turn.clone());
                                    thread.notify(json!({"method":"turn/started","params":{"threadId":native,"turn":turn}}));
                                    json!({"turn":turn})
                                }
                                _ => panic!("unexpected method {method}"),
                            };
                            json!({"id":id,"result":result})
                        };
                        drop(threads);
                        if method == "thread/fork" && faults.lock().await.remove("drop_fork_response") { break; }
                        if wire.send(Message::text(response.to_string())).await.is_err() { break; }
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
        let (shutdown, stop) = watch::channel(false);
        let observer = CodexObserver::new(&app, root.path().into());
        observer.prepare().await.unwrap();
        tasks.spawn(observer.run(stop.clone()));
        let proxy = Proxy::bind(root.path()).await.unwrap();
        let (observations, incoming) = mpsc::channel(128);
        tasks.spawn(async move {
            proxy
                .run(stop, move || Ok(socket.clone()), observations)
                .await
                .unwrap();
        });
        tasks.spawn(run(app.clone(), incoming));
        Self {
            app,
            root,
            threads,
            faults,
            tasks,
            shutdown,
        }
    }

    async fn client(&self) -> crate::runtime::protocol::Socket {
        let mut wire = crate::runtime::protocol::open(&crate::proxy::socket_path(self.root.path()))
            .await
            .unwrap();
        rpc(
            &mut wire,
            "initialize",
            json!({"clientInfo":{"name":"cli"}}),
        )
        .await;
        wire
    }

    async fn session(&self, thread: &str, expected: &str) -> String {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let row: Option<(String, String)> = sqlx::query_as("SELECT s.session_id,s.state FROM sessions s JOIN agent_bindings a USING(session_id) WHERE a.client_session_key=?")
                    .bind(thread).fetch_optional(&self.app.db()).await.unwrap();
                if let Some((id, state)) = row && state == expected { return id; }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        }).await.unwrap()
    }

    async fn complete(&self, native: &str) {
        let mut threads = self.threads.lock().await;
        let thread = threads.get_mut(native).unwrap();
        let turn = thread.turns.last_mut().unwrap();
        turn["status"] = json!("completed");
        turn["items"].as_array_mut().unwrap().push(
            json!({"id":"answer","type":"agentMessage","text":"done","phase":"final_answer"}),
        );
        let turn = turn.clone();
        thread.notify(json!({"method":"turn/completed","params":{"threadId":native,"turn":turn}}));
    }

    async fn stop(mut self) {
        self.shutdown.send(true).unwrap();
        CodexRuntime::shutdown(self.root.path()).await;
        self.tasks.shutdown().await;
        self.app.db().close().await;
    }
}

async fn rpc(wire: &mut crate::runtime::protocol::Socket, method: &str, params: Value) -> Value {
    wire.send(Message::text(
        json!({"id":1,"method":method,"params":params}).to_string(),
    ))
    .await
    .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let frame = wire.next().await.unwrap().unwrap();
            if let Message::Text(text) = frame {
                let value: Value = serde_json::from_str(&text).unwrap();
                if value["id"] == 1 && value.get("method").is_none() {
                    return value;
                }
            }
        }
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn cli_start_and_first_input_join_existing_turn_observation() {
    let f = Fixture::new().await;
    let mut cli = f.client().await;
    let response = rpc(&mut cli, "thread/start", json!({})).await;
    let native = response["result"]["thread"]["id"].as_str().unwrap();
    let session = f.session(native, "idle").await;
    rpc(&mut cli, "turn/start", json!({"threadId":native})).await;
    assert_eq!(f.session(native, "busy").await, session);
    let turn_id: String = sqlx::query_scalar("SELECT turn_id FROM turns WHERE session_id=?")
        .bind(&session)
        .fetch_one(&f.app.db())
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while f.app.live_output().snapshot(&session, &turn_id).is_none() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    {
        let threads = f.threads.lock().await;
        threads[native].notify(json!({"method":"item/agentMessage/delta","params":{"threadId":native,"turnId":"turn-0","itemId":"answer","delta":"live from CLI"}}));
    }
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Some(snapshot) = f.app.live_output().snapshot(&session, &turn_id)
                && snapshot.items
                    == vec![pontia_application::LiveOutputItem::AssistantText {
                        item_id: "answer".into(),
                        text: "live from CLI".into(),
                    }]
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    f.complete(native).await;
    assert_eq!(f.session(native, "idle").await, session);
    let turns: Vec<(String, String)> =
        sqlx::query_as("SELECT state,input_summary FROM turns WHERE session_id=?")
            .bind(&session)
            .fetch_all(&f.app.db())
            .await
            .unwrap();
    assert_eq!(turns, vec![("completed".into(), "from CLI".into())]);
    drop(cli);
    assert_eq!(f.session(native, "idle").await, session);
    f.stop().await;
}

#[tokio::test]
async fn fork_creates_a_distinct_session_and_concurrent_resume_reuses_identity() {
    let f = Fixture::new().await;
    let mut cli = f.client().await;
    let response = rpc(&mut cli, "thread/start", json!({})).await;
    let native = response["result"]["thread"]["id"].as_str().unwrap();
    let original = f.session(native, "idle").await;
    let fork = rpc(&mut cli, "thread/fork", json!({"threadId":native})).await;
    let branch = fork["result"]["thread"]["id"].as_str().unwrap();
    let session = f.session(branch, "idle").await;
    assert_ne!(session, original);
    let mut other = f.client().await;
    tokio::join!(
        rpc(&mut cli, "thread/resume", json!({"threadId":branch})),
        rpc(&mut other, "thread/resume", json!({"threadId":branch})),
    );
    // A later input proves both preceding resume receipts were processed.
    rpc(&mut cli, "turn/start", json!({"threadId":branch})).await;
    assert_eq!(f.session(branch, "busy").await, session);
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions")
        .fetch_one(&f.app.db())
        .await
        .unwrap();
    assert_eq!(count, 2);
    f.stop().await;
}

#[tokio::test]
async fn dashboard_fork_excludes_the_target_and_preserves_running_source() {
    let f = Fixture::new().await;
    let mut cli = f.client().await;
    let response = rpc(&mut cli, "thread/start", json!({})).await;
    let native = response["result"]["thread"]["id"].as_str().unwrap();
    let source = f.session(native, "idle").await;
    for _ in 0..2 {
        rpc(&mut cli, "turn/start", json!({"threadId":native})).await;
        f.session(native, "busy").await;
        f.complete(native).await;
        f.session(native, "idle").await;
    }
    rpc(&mut cli, "turn/start", json!({"threadId":native})).await;
    f.session(native, "busy").await;
    let target: String = sqlx::query_scalar(
        "SELECT turn_id FROM native_turn_bindings WHERE session_id=? AND client_turn_id='turn-1'",
    )
    .bind(&source)
    .fetch_one(&f.app.db())
    .await
    .unwrap();
    let request = || pontia_application::sessions::ForkSessionRequest {
        operation_id: "edit-middle".into(),
        turn_id: target.clone(),
    };
    let result = f
        .app
        .session_commands()
        .fork_session(&source, request())
        .await
        .unwrap();
    let child = result.session_id.unwrap();
    let repeated = f
        .app
        .session_commands()
        .fork_session(&source, request())
        .await
        .unwrap();
    assert_eq!(repeated.session_id.as_deref(), Some(child.as_str()));
    assert_eq!(f.session(native, "busy").await, source);
    let binding = AgentBindingService::new(f.app.db())
        .binding_for_session(&child)
        .await
        .unwrap()
        .unwrap();
    assert_ne!(child, source);
    assert_eq!(
        f.threads.lock().await[&binding.client_session_key]
            .turns
            .iter()
            .map(|turn| turn["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["turn-0"]
    );
    let lineage: (String, String, String) = sqlx::query_as("SELECT parent_session_id,forked_from_turn_id,json_extract(metadata,'$.boundary') FROM session_lineage WHERE child_session_id=?")
        .bind(&child).fetch_one(&f.app.db()).await.unwrap();
    assert_eq!(lineage, (source.clone(), target, "before".into()));
    f.app
        .inbox_commands()
        .submit_message_once(
            "replacement",
            &child,
            pontia_application::SubmitInboxMessageRequest {
                input: "edited prompt".into(),
                delivery_policy: "after_idle".into(),
                branch_target_turn_id: None,
                metadata: json!({}),
            },
        )
        .await
        .unwrap();
    f.app.inbox_commands().drain_inbox(&child).await.unwrap();
    f.session(&binding.client_session_key, "busy").await;
    f.complete(&binding.client_session_key).await;
    f.session(&binding.client_session_key, "idle").await;
    let inputs: Vec<String> =
        sqlx::query_scalar("SELECT input_summary FROM turns WHERE session_id=? ORDER BY turn_id")
            .bind(&child)
            .fetch_all(&f.app.db())
            .await
            .unwrap();
    assert_eq!(inputs, ["from CLI", "edited prompt"]);
    assert_eq!(f.threads.lock().await.len(), 2);
    f.stop().await;
}

#[tokio::test]
async fn cli_fork_inherits_history_and_resume_keeps_its_boundary() {
    let f = Fixture::new().await;
    let mut cli = f.client().await;
    let response = rpc(&mut cli, "thread/start", json!({})).await;
    let native = response["result"]["thread"]["id"].as_str().unwrap();
    let source = f.session(native, "idle").await;
    for _ in 0..2 {
        rpc(&mut cli, "turn/start", json!({"threadId":native})).await;
        f.session(native, "busy").await;
        f.complete(native).await;
        f.session(native, "idle").await;
    }
    let fork = rpc(
        &mut cli,
        "thread/fork",
        json!({"threadId":native,"beforeTurnId":"turn-1"}),
    )
    .await;
    let native_child = fork["result"]["thread"]["id"].as_str().unwrap();
    let child = f.session(native_child, "idle").await;
    rpc(&mut cli, "thread/resume", json!({"threadId":native_child})).await;
    rpc(&mut cli, "turn/start", json!({"threadId":native_child})).await;
    f.session(native_child, "busy").await;
    let lineage: (String, String) = sqlx::query_as("SELECT parent_session_id,forked_from_client_node_id FROM session_lineage WHERE child_session_id=?")
        .bind(&child).fetch_one(&f.app.db()).await.unwrap();
    assert_eq!(lineage, (source, "turn-1".into()));
    let turns: Vec<(String, String)> = sqlx::query_as("SELECT b.client_turn_id,t.state FROM native_turn_bindings b JOIN turns t ON b.turn_id=t.turn_id WHERE b.session_id=? ORDER BY t.turn_id")
        .bind(&child).fetch_all(&f.app.db()).await.unwrap();
    assert_eq!(
        turns,
        [
            ("turn-0".into(), "completed".into()),
            ("turn-1".into(), "running".into())
        ]
    );
    f.stop().await;
}

#[tokio::test]
async fn first_turn_fork_is_empty_and_active_turn_is_rejected() {
    let f = Fixture::new().await;
    let mut cli = f.client().await;
    let response = rpc(&mut cli, "thread/start", json!({})).await;
    let native = response["result"]["thread"]["id"].as_str().unwrap();
    let source = f.session(native, "idle").await;
    rpc(&mut cli, "turn/start", json!({"threadId":native})).await;
    f.session(native, "busy").await;
    let target: String = sqlx::query_scalar("SELECT turn_id FROM turns WHERE session_id=?")
        .bind(&source)
        .fetch_one(&f.app.db())
        .await
        .unwrap();
    let request = || pontia_application::sessions::ForkSessionRequest {
        operation_id: "edit-first".into(),
        turn_id: target.clone(),
    };
    assert!(
        f.app
            .session_commands()
            .fork_session(&source, request())
            .await
            .is_err()
    );
    assert_eq!(f.threads.lock().await.len(), 1);
    f.complete(native).await;
    f.session(native, "idle").await;
    let result = f
        .app
        .session_commands()
        .fork_session(&source, request())
        .await
        .unwrap();
    let child = result.session_id.unwrap();
    let turns: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM turns WHERE session_id=?")
        .bind(&child)
        .fetch_one(&f.app.db())
        .await
        .unwrap();
    assert_eq!(turns, 0);
    assert_eq!(f.threads.lock().await.len(), 2);
    f.stop().await;
}

#[tokio::test]
async fn attachment_retry_reuses_the_confirmed_native_fork_after_command_service_recreation() {
    let f = Fixture::new().await;
    let mut cli = f.client().await;
    let response = rpc(&mut cli, "thread/start", json!({})).await;
    let native = response["result"]["thread"]["id"].as_str().unwrap();
    let source = f.session(native, "idle").await;
    rpc(&mut cli, "turn/start", json!({"threadId":native})).await;
    f.session(native, "busy").await;
    f.complete(native).await;
    f.session(native, "idle").await;
    let target: String = sqlx::query_scalar("SELECT turn_id FROM turns WHERE session_id=?")
        .bind(&source)
        .fetch_one(&f.app.db())
        .await
        .unwrap();
    let request = || pontia_application::sessions::ForkSessionRequest {
        operation_id: "recover-adoption".into(),
        turn_id: target.clone(),
    };
    f.faults.lock().await.insert("thread/resume".into());
    let partial = f
        .app
        .session_commands()
        .fork_session(&source, request())
        .await
        .unwrap();
    assert_eq!(
        serde_json::to_value(&partial).unwrap()["stage"],
        "session_pending"
    );
    assert!(partial.session_id.is_some());
    assert_eq!(f.threads.lock().await.len(), 2);
    f.faults.lock().await.clear();
    let binding = AgentBindingService::new(f.app.db())
        .binding_for_session(partial.session_id.as_deref().unwrap())
        .await
        .unwrap()
        .unwrap();
    f.threads
        .lock()
        .await
        .get_mut(&binding.client_session_key)
        .unwrap()
        .value["status"]["type"] = json!("notLoaded");
    let restarted = AppState::builder(f.app.db(), f.root.path().into())
        .clients(f.app.clients())
        .build();
    let recovered = restarted
        .session_commands()
        .fork_session(&source, request())
        .await
        .unwrap();
    assert_eq!(
        serde_json::to_value(&recovered).unwrap()["stage"],
        "session_created"
    );
    assert_eq!(recovered.session_id, partial.session_id);
    assert_eq!(f.threads.lock().await.len(), 2);
    f.stop().await;
}

#[tokio::test]
async fn native_rejection_and_lost_receipt_do_not_repeat_forks() {
    for (fault, stage, count) in [
        ("thread/fork", "fork_failed", 1),
        ("drop_fork_response", "fork_unconfirmed", 2),
    ] {
        let f = Fixture::new().await;
        let mut cli = f.client().await;
        let response = rpc(&mut cli, "thread/start", json!({})).await;
        let native = response["result"]["thread"]["id"].as_str().unwrap();
        let source = f.session(native, "idle").await;
        rpc(&mut cli, "turn/start", json!({"threadId":native})).await;
        f.session(native, "busy").await;
        f.complete(native).await;
        f.session(native, "idle").await;
        let target: String = sqlx::query_scalar("SELECT turn_id FROM turns WHERE session_id=?")
            .bind(&source)
            .fetch_one(&f.app.db())
            .await
            .unwrap();
        let request = || pontia_application::sessions::ForkSessionRequest {
            operation_id: "failure".into(),
            turn_id: target.clone(),
        };
        f.faults.lock().await.insert(fault.into());
        let result = f
            .app
            .session_commands()
            .fork_session(&source, request())
            .await
            .unwrap();
        assert_eq!(serde_json::to_value(&result).unwrap()["stage"], stage);
        assert!(result.session_id.is_none());
        f.faults.lock().await.clear();
        let repeated = f
            .app
            .session_commands()
            .fork_session(&source, request())
            .await
            .unwrap();
        assert_eq!(serde_json::to_value(&repeated).unwrap()["stage"], stage);
        assert_eq!(f.threads.lock().await.len(), count);
        let sessions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions")
            .fetch_one(&f.app.db())
            .await
            .unwrap();
        assert_eq!(sessions, 1);
        f.stop().await;
    }
}

#[tokio::test]
async fn rejected_and_ephemeral_threads_are_not_adopted() {
    let f = Fixture::new().await;
    let mut cli = f.client().await;
    assert!(
        rpc(&mut cli, "thread/start", json!({"reject":true}))
            .await
            .get("error")
            .is_some()
    );
    rpc(&mut cli, "thread/start", json!({"ephemeral":true})).await;
    let response = rpc(&mut cli, "thread/start", json!({})).await;
    f.session(response["result"]["thread"]["id"].as_str().unwrap(), "idle")
        .await;
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions")
        .fetch_one(&f.app.db())
        .await
        .unwrap();
    assert_eq!(count, 1);
    f.stop().await;
}

#[tokio::test]
async fn cli_resume_restores_exited_session_without_changing_turn_history() {
    let f = Fixture::new().await;
    let mut cli = f.client().await;
    let response = rpc(&mut cli, "thread/fork", json!({})).await;
    let native = response["result"]["thread"]["id"].as_str().unwrap();
    let session = f.session(native, "idle").await;
    rpc(&mut cli, "turn/start", json!({"threadId":native})).await;
    f.session(native, "busy").await;
    f.complete(native).await;
    f.session(native, "idle").await;
    {
        let mut threads = f.threads.lock().await;
        let thread = threads.get_mut(native).unwrap();
        thread.value["status"]["type"] = json!("notLoaded");
        thread.notify(json!({"method":"thread/status/changed","params":{"threadId":native,"status":{"type":"notLoaded"}}}));
    }
    f.app
        .event_ingest_service()
        .report_client_fact(
            &session,
            None,
            pontia_core::domain::EventType::SessionExited,
            json!({"reason":"thread_unloaded"}),
        )
        .await
        .unwrap();
    confirmed_exits().lock().await.insert(session.clone());
    rpc(&mut cli, "thread/resume", json!({"threadId":native})).await;
    assert_eq!(f.session(native, "idle").await, session);
    rpc(&mut cli, "turn/start", json!({"threadId":native})).await;
    f.session(native, "busy").await;
    f.complete(native).await;
    f.session(native, "idle").await;
    let events: Vec<String> = sqlx::query_scalar("SELECT event_type FROM events WHERE session_id=? AND event_type IN ('session.created','session.resuming','turn.completed') ORDER BY rowid")
        .bind(&session).fetch_all(&f.app.db()).await.unwrap();
    assert_eq!(
        events,
        [
            "session.created",
            "turn.completed",
            "session.resuming",
            "turn.completed"
        ]
    );
    f.stop().await;
}

#[tokio::test]
async fn delayed_resume_receipt_does_not_reopen_a_thread_unloaded_again() {
    let f = Fixture::new().await;
    let mut cli = f.client().await;
    let response = rpc(&mut cli, "thread/fork", json!({})).await;
    let native = response["result"]["thread"]["id"].as_str().unwrap();
    let session = f.session(native, "idle").await;
    f.app
        .event_ingest_service()
        .report_client_fact(
            &session,
            None,
            pontia_core::domain::EventType::SessionExited,
            json!({"reason":"thread_unloaded"}),
        )
        .await
        .unwrap();
    let runtime = CodexRuntime::existing(f.root.path()).await.unwrap();
    let operation = runtime.lock_session(&session).await;
    rpc(&mut cli, "thread/resume", json!({"threadId":native})).await;
    {
        let mut threads = f.threads.lock().await;
        let thread = threads.get_mut(native).unwrap();
        thread.value["status"]["type"] = json!("notLoaded");
        thread.notify(json!({"method":"thread/status/changed","params":{"threadId":native,"status":{"type":"notLoaded"}}}));
    }
    drop(operation);
    // This later receipt is processed after the stale resume on the same proxy connection.
    let next = rpc(&mut cli, "thread/fork", json!({"threadId":native})).await;
    f.session(next["result"]["thread"]["id"].as_str().unwrap(), "idle")
        .await;
    assert_eq!(f.session(native, "exited").await, session);
    let resumed: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM events WHERE session_id=? AND event_type='session.resuming'",
    )
    .bind(&session)
    .fetch_one(&f.app.db())
    .await
    .unwrap();
    assert_eq!(resumed, 0);
    assert_eq!(
        f.threads.lock().await[native].value["status"]["type"],
        "notLoaded"
    );
    f.stop().await;
}
