use super::*;
use futures_util::{SinkExt, StreamExt};
use pontia_application::{
    AppState, CreateSessionRequest, ExternalQueryService, clients::ClientRegistry,
};
use serde_json::json;
use std::time::Duration;
use tokio::net::UnixListener;
use tokio_tungstenite::{accept_async, tungstenite::Message};

mod lifecycle;
mod live;

#[derive(Default)]
struct ServerState {
    threads: Vec<Value>,
    turns: HashMap<String, Value>,
    archived: Vec<String>,
    publish_turns: bool,
    stall_turn_reads: bool,
}

struct RuntimeGuard(Arc<CodexRuntime>);
impl Drop for RuntimeGuard {
    fn drop(&mut self) {
        if let Ok(mut runtimes) = registry().try_lock() {
            runtimes.remove(&self.0.root);
        }
    }
}

#[tokio::test]
async fn shared_server_keeps_control_receipts_sessions_and_tui_lifetimes_separate() {
    let root = tempfile::tempdir().unwrap();
    let socket = root.path().join("native.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let native = Arc::new(Mutex::new(ServerState::default()));
    let server_state = native.clone();
    let read_started = Arc::new(tokio::sync::Notify::new());
    let release_read = Arc::new(tokio::sync::Notify::new());
    let server_read_started = read_started.clone();
    let server_release_read = release_read.clone();
    let cwd = root.path().display().to_string();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut wire = accept_async(stream).await.unwrap();
        while let Some(Ok(Message::Text(frame))) = wire.next().await {
            let request: Value = serde_json::from_str(&frame).unwrap();
            if request.get("id").is_none() {
                continue;
            }
            let mut state = server_state.lock().await;
            let thread = request["params"]["threadId"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            let result = match request["method"].as_str().unwrap() {
                "initialize" => {
                    json!({"userAgent":"pontia/0.156.1", "codexHome":cwd,"platformFamily":"unix","platformOs":"linux"})
                }
                "thread/start" => {
                    let id = format!("thread-{}", state.threads.len());
                    let thread = json!({"id":id,"cwd":cwd,"canAcceptDirectInput":true,"status":{"type":"idle"}});
                    state.threads.push(thread.clone());
                    json!({"thread":thread,"model":"test-model"})
                }
                "thread/read" | "thread/resume" => {
                    json!({"thread":state.threads.iter().find(|entry| entry["id"] == thread).unwrap(),"model":"test-model"})
                }
                "thread/turns/list" => {
                    if state.stall_turn_reads {
                        server_read_started.notify_one();
                        server_release_read.notified().await;
                    }
                    json!({"data":if state.publish_turns { state.turns.get(&thread).cloned().into_iter().collect::<Vec<_>>() } else { vec![] }, "nextCursor":null})
                }
                "turn/start" => {
                    let id = format!("turn-{thread}");
                    state.turns.insert(thread, json!({"id":id,"status":"inProgress","items":[{"type":"userMessage","content":[{"text":request["params"]["input"][0]["text"]}]}]}));
                    json!({"turn":{"id":id}})
                }
                "thread/archive" => {
                    state.archived.push(thread);
                    json!({})
                }
                "thread/unarchive" => {
                    state.archived.retain(|id| id != &thread);
                    json!({})
                }
                "thread/list" => {
                    json!({"data":state.archived.iter().map(|id| json!({"id":id})).collect::<Vec<_>>(),"nextCursor":null})
                }
                method => panic!("unexpected method: {method}"),
            };
            if wire
                .send(Message::Text(
                    json!({"id":request["id"],"result":result})
                        .to_string()
                        .into(),
                ))
                .await
                .is_err()
            {
                break;
            }
        }
    });
    let connection = Connection::connect(&socket).await.unwrap();
    let runtime = Arc::new(CodexRuntime {
        root: root.path().canonicalize().unwrap(),
        instance_id: connection.identity.instance_id.clone(),
        connection_id: "connection-test".into(),
        socket_path: socket.clone(),
        connection,
        targets: broadcast::channel(128).0,
        gateways: Mutex::new(HashMap::new()),
        operations: Mutex::new(HashMap::new()),
        tui_targets: Mutex::new(HashMap::new()),
    });
    let _runtime_guard = RuntimeGuard(runtime.clone());
    registry()
        .lock()
        .await
        .insert(runtime.root.clone(), runtime.clone());
    let pool = pontia_storage_sqlite::connect_sqlite(&format!(
        "sqlite://{}",
        root.path().join("test.db").display()
    ))
    .await
    .unwrap();
    pontia_storage_sqlite::run_migrations(&pool).await.unwrap();
    let mut clients = ClientRegistry::default();
    clients.register(crate::registration());
    let app = AppState::builder(pool, root.path().into())
        .clients(clients)
        .build();
    let sessions = app.session_commands();
    let turns = app.turn_commands();
    let query =
        ExternalQueryService::new(app.db()).with_clients(app.event_ingest_service().clients());
    let mut ids = Vec::new();
    for input in ["first", "second"] {
        let request: CreateSessionRequest =
            serde_json::from_value(json!({"client_type":"codex","workspace":root.path()})).unwrap();
        let created = sessions.create_session(request).await.unwrap();
        let id = created.session_id().unwrap().to_owned();
        assert_eq!(
            query.get_session(&id).await.unwrap().unwrap().state,
            "created"
        );
        // Existing interface ownership avoids launching a real native TUI in this test.
        sqlx::query("INSERT INTO codex_tui_bindings(owner_session_id,target_session_id,runtime_instance_id,connected,connection_id) VALUES (?,?,?,TRUE,'ui')")
            .bind(&id).bind(&id).bind(&runtime.instance_id).execute(&app.db()).await.unwrap();
        assert!(
            turns
                .create_and_dispatch_turn(&id, input.into(), json!({}))
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            query.list_turns(&id).await.unwrap().is_empty(),
            "acceptance is not a Turn fact"
        );
        ids.push(id);
    }
    assert!(Arc::ptr_eq(
        &runtime,
        &CodexRuntime::ensure(root.path()).await.unwrap()
    ));
    assert_eq!(native.lock().await.threads.len(), 2);
    native.lock().await.publish_turns = true;
    let (shutdown, receiver) = tokio::sync::watch::channel(false);
    let observer = tokio::spawn(
        crate::CodexObserver::new(app.event_ingest_service(), root.path().into()).run(receiver),
    );
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if query.list_turns(&ids[0]).await.unwrap().len() == 1
                && query.list_turns(&ids[1]).await.unwrap().len() == 1
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let second_turn = query.list_turns(&ids[1]).await.unwrap().remove(0);
    assert_eq!(second_turn.input.summary.as_deref(), Some("second"));
    runtime
        .targets
        .send(TuiTarget {
            connection_id: "ui".into(),
            owner_session_id: ids[1].clone(),
            thread: json!({"id":"thread-1"}),
            connected: false,
        })
        .unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let connected: bool = sqlx::query_scalar(
                "SELECT connected FROM codex_tui_bindings WHERE owner_session_id=?",
            )
            .bind(&ids[1])
            .fetch_one(&app.db())
            .await
            .unwrap();
            if !connected {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        query.get_session(&ids[1]).await.unwrap().unwrap().state,
        "busy"
    );
    sessions.terminate_session(&ids[0]).await.unwrap();
    assert_eq!(
        query.get_session(&ids[0]).await.unwrap().unwrap().state,
        "exited"
    );
    assert_eq!(
        query.get_session(&ids[1]).await.unwrap().unwrap().state,
        "busy"
    );
    assert_eq!(query.list_turns(&ids[0]).await.unwrap()[0].state, "running");
    assert!(Arc::ptr_eq(
        &runtime,
        &CodexRuntime::ensure(root.path()).await.unwrap()
    ));
    native.lock().await.turns.get_mut("thread-0").unwrap()["status"] = json!("completed");
    sessions.resume_session(&ids[0], root.path()).await.unwrap();
    assert_eq!(
        query.get_session(&ids[0]).await.unwrap().unwrap().state,
        "idle"
    );
    assert_eq!(
        native.lock().await.threads.len(),
        2,
        "resume retains native identity"
    );
    native.lock().await.stall_turn_reads = true;
    tokio::time::timeout(Duration::from_secs(5), read_started.notified())
        .await
        .unwrap();
    shutdown.send(true).unwrap();
    tokio::time::timeout(Duration::from_secs(1), observer)
        .await
        .unwrap()
        .unwrap();
    assert!(
        runtime.connection.is_connected(),
        "runtime cleanup follows observer completion"
    );
    let status: String = sqlx::query_scalar("SELECT json_extract(adapter_details,'$.codex.connection') FROM runtime_bindings WHERE session_id=?")
        .bind(&ids[0]).fetch_one(&app.db()).await.unwrap();
    assert_eq!(status, "unavailable");
    release_read.notify_one();
    CodexRuntime::shutdown(root.path()).await;
    assert!(!runtime.connection.is_connected());
    assert!(socket.exists(), "external daemon socket is never removed");
    assert!(!registry().lock().await.contains_key(&runtime.root));
    // A delayed response from the removed runtime cannot restore its binding.
    let old_binding = pontia_application::AgentBindingService::new(app.db())
        .binding_for_session(&ids[0])
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        crate::CodexService::new(app.event_ingest_service())
            .bind(
                &ids[0],
                &runtime,
                &json!({"id":"thread-0","cwd":root.path(),"path":"obsolete-rollout"}),
                Some(&runtime.instance_id)
            )
            .await,
        Err(Error::StateConflict(_))
    ));
    assert_eq!(
        pontia_application::AgentBindingService::new(app.db())
            .binding_for_session(&ids[0])
            .await
            .unwrap()
            .unwrap(),
        old_binding
    );
    server.await.unwrap();
}
