use super::*;
use pontia_application::{AgentBindingService, runtime::ControlTarget};

async fn wait_turns(query: &ExternalQueryService, session: &str, count: usize) {
    tokio::time::timeout(Duration::from_secs(90), async {
        loop {
            let turns = query.list_turns(session).await.unwrap();
            if turns.len() == count && turns.iter().all(|turn| turn.state == "completed") {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .unwrap();
}

// Opt-in: uses an externally started daemon and its credentials. Creates only its
// own thread, with a temporary workspace/database. Never starts or stops a daemon.
#[tokio::test]
#[ignore = "requires an externally running Codex 0.156.1 daemon and model access"]
async fn external_daemon_two_clients_reconcile_and_survive_pontia_shutdown() {
    let root = tempfile::tempdir().unwrap();
    let runtime = CodexRuntime::ensure(root.path()).await.unwrap();
    let endpoint = runtime.socket_path.clone();
    let identity = runtime.instance_id.clone();
    let external = Connection::connect(&endpoint).await.unwrap();
    assert_eq!(external.identity.instance_id, identity);
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
    let created = app
        .session_commands()
        .create_session(
            serde_json::from_value(json!({"client_type":"codex","workspace":root.path()})).unwrap(),
        )
        .await
        .unwrap();
    let session = created.session_id().unwrap().to_owned();
    // Reserve UI ownership so this test can attach through the gateway itself.
    sqlx::query("INSERT INTO codex_tui_bindings(owner_session_id,target_session_id,runtime_instance_id,connected,connection_id) VALUES(?,?,?,TRUE,'live')")
        .bind(&session).bind(&session).bind(&identity).execute(&app.db()).await.unwrap();
    app.turn_commands()
        .create_and_dispatch_turn(
            &session,
            "Reply only PONTIA_FIRST_OK. Do not use tools.".into(),
            json!({}),
        )
        .await
        .unwrap();
    let thread = AgentBindingService::new(app.db())
        .binding_for_session(&session)
        .await
        .unwrap()
        .unwrap()
        .client_session_key;
    let query =
        ExternalQueryService::new(app.db()).with_clients(app.event_ingest_service().clients());
    let (stop, receiver) = tokio::sync::watch::channel(false);
    let observer = tokio::spawn(
        crate::CodexObserver::new(app.event_ingest_service(), root.path().into()).run(receiver),
    );
    wait_turns(&query, &session, 1).await;
    let gateway = runtime.gateway(&session).await.unwrap();
    let tui = Connection::connect(Path::new(gateway.trim_start_matches("unix://")))
        .await
        .unwrap();
    tui.call(
        "thread/resume",
        json!({"threadId":thread,"excludeTurns":true}),
    )
    .await
    .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let target = runtime.tui_targets.lock().await.get(&session).cloned();
            if target.is_some_and(|target| target.connected && target.thread["id"] == thread) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    external
        .call(
            "thread/resume",
            json!({"threadId":thread,"excludeTurns":true}),
        )
        .await
        .unwrap();
    external.call("turn/start", json!({"threadId":thread,"input":[{"type":"text","text":"Reply only PONTIA_EXTERNAL_OK. Do not use tools."}]})).await.unwrap();
    wait_turns(&query, &session, 2).await;
    let service = crate::CodexService::new(app.event_ingest_service());
    let target = ControlTarget::resolve(&app.db(), &session, Some(&identity))
        .await
        .unwrap();
    let models = service.list_models(&target).await.unwrap();
    assert!(!models.is_empty());
    let model = query.get_session(&session).await.unwrap().unwrap().metadata["model"]
        .as_str()
        .unwrap()
        .to_string();
    service.set_model(&target, &model).await.unwrap();
    // Disconnect Pontia while another client is executing. The thread and daemon
    // remain live; the next observer must compensate for the entire missed turn.
    stop.send(true).unwrap();
    observer.await.unwrap();
    external.call("turn/start", json!({"threadId":thread,"input":[{"type":"text","text":"Reply only PONTIA_SURVIVED_OK. Do not use tools."}]})).await.unwrap();
    CodexRuntime::shutdown(root.path()).await;
    assert!(endpoint.exists());
    assert!(external.is_connected());
    tokio::time::timeout(Duration::from_secs(5), async {
        while tui.is_connected() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let replacement = CodexRuntime::ensure(root.path()).await.unwrap();
    assert_eq!(replacement.instance_id, identity);
    assert!(runtime.current_guard().await.is_err());
    let (stop, receiver) = tokio::sync::watch::channel(false);
    let observer = tokio::spawn(
        crate::CodexObserver::new(app.event_ingest_service(), root.path().into()).run(receiver),
    );
    wait_turns(&query, &session, 3).await;
    let native = external
        .call(
            "thread/turns/list",
            json!({"threadId":thread,"limit":100,"sortDirection":"asc","itemsView":"full"}),
        )
        .await
        .unwrap();
    assert_eq!(native["data"].as_array().unwrap().len(), 3);
    external.call("turn/start", json!({"threadId":thread,"input":[{"type":"text","text":"Write the integers 1 through 10000 separated by spaces. Do not use tools."}]})).await.unwrap();
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            if query
                .list_turns(&session)
                .await
                .unwrap()
                .iter()
                .any(|turn| turn.state == "running")
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    app.turn_commands()
        .interrupt_current_turn(&session)
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let turns = query.list_turns(&session).await.unwrap();
            if turns.len() == 4 && turns.iter().any(|turn| turn.state == "interrupted") {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    if std::env::var_os("PONTIA_CODEX_TEST_WAIT_FOR_RESTART").is_some() {
        eprintln!("READY_FOR_EXTERNAL_DAEMON_RESTART thread={thread}");
        tokio::time::timeout(Duration::from_secs(120), async {
            loop {
                let (instance, state): (String, String) = sqlx::query_as("SELECT runtime_instance_id,json_extract(adapter_details,'$.codex.connection') FROM runtime_bindings WHERE session_id=?")
                    .bind(&session).fetch_one(&app.db()).await.unwrap();
                if instance != identity && state == "available" { break; }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }).await.unwrap();
        assert!(target.validate(&app.db()).await.is_err());
        assert_eq!(query.list_turns(&session).await.unwrap().len(), 4);
        let binding = AgentBindingService::new(app.db())
            .binding_for_session(&session)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(binding.client_session_key, thread);
        let new_external = Connection::connect(&endpoint).await.unwrap();
        assert_ne!(new_external.identity.instance_id, identity);
        new_external.close().await;
        eprintln!("Recovered persistent Session and 4 distinct Turns after daemon restart");
    }
    app.session_commands()
        .terminate_session(&session)
        .await
        .unwrap();
    assert_eq!(
        query.get_session(&session).await.unwrap().unwrap().state,
        "exited"
    );
    if std::env::var_os("PONTIA_CODEX_TEST_WAIT_FOR_RESTART").is_none() {
        assert!(external.is_connected());
    }
    stop.send(true).unwrap();
    observer.await.unwrap();
    CodexRuntime::shutdown(root.path()).await;
    external.close().await;
    assert!(endpoint.exists());
    eprintln!(
        "Validated daemon {identity}, thread {thread}: two clients, model control, shutdown, history compensation, archive"
    );
}
