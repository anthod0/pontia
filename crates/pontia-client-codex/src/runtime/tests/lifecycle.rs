use super::*;
use tokio::process::Command;

// Runs in a child test process so SO_PEERCRED observes a real, distinct peer.
#[tokio::test]
#[ignore = "subprocess fixture; invoked by the daemon replacement test"]
async fn daemon_peer_fixture() {
    let Some(root) = std::env::var_os("PONTIA_TEST_DAEMON_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    let listener = UnixListener::bind(root.join("daemon.sock")).unwrap();
    loop {
        let (stream, _) = listener.accept().await.unwrap();
        let root = root.clone();
        tokio::spawn(async move {
            let mut wire = accept_async(stream).await.unwrap();
            while let Some(Ok(Message::Text(frame))) = wire.next().await {
                let request: Value = serde_json::from_str(&frame).unwrap();
                if request.get("id").is_none() {
                    continue;
                }
                let result = if request["method"] == "initialize" {
                    json!({"userAgent":"pontia/0.156.1","codexHome":root,"platformFamily":"unix","platformOs":"linux"})
                } else {
                    json!({})
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
    }
}

async fn fixture(root: &Path) -> tokio::process::Child {
    let child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "runtime::tests::lifecycle::daemon_peer_fixture",
            "--ignored",
        ])
        .env("PONTIA_TEST_DAEMON_ROOT", root)
        .stdout(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while !root.join("daemon.sock").exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    child
}

async fn connect(root: &Path) -> Result<Arc<CodexRuntime>> {
    CodexRuntime::connect(
        &mut *registry().lock().await,
        root.canonicalize().unwrap(),
        daemon::Endpoint {
            socket: root.join("daemon.sock"),
            home: root.canonicalize().unwrap(),
        },
    )
    .await
}

#[tokio::test]
async fn external_peer_replacement_and_gateway_shutdown_are_isolated() {
    let root = tempfile::tempdir().unwrap();
    let mut daemon = fixture(root.path()).await;
    let runtime = connect(root.path()).await.unwrap();
    let original = runtime.instance_id.clone();
    let gateway = runtime.gateway("sess_test").await.unwrap();
    let gateway_path = PathBuf::from(gateway.trim_start_matches("unix://"));
    let tui = Connection::connect(&gateway_path).await.unwrap();
    let mut closed = tui.events.subscribe();
    CodexRuntime::shutdown(root.path()).await;
    tokio::time::timeout(Duration::from_secs(2), async {
        while closed.recv().await.unwrap()["method"] != "pontia/disconnected" {}
    })
    .await
    .unwrap();
    assert!(!gateway_path.exists());
    assert!(root.path().join("daemon.sock").exists());
    assert!(daemon.try_wait().unwrap().is_none());
    assert!(runtime.gateway("sess_stale").await.is_err());
    let reconnected = connect(root.path()).await.unwrap();
    assert_eq!(reconnected.instance_id, original);
    let mut disconnected = reconnected.connection.events.subscribe();
    daemon.kill().await.unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        while disconnected.recv().await.unwrap()["method"] != "pontia/disconnected" {}
    })
    .await
    .unwrap();
    assert!(
        reconnected.connection().await.is_err(),
        "a stale runtime never silently reconnects"
    );
    // The fixture owns this temporary socket; Pontia must leave it intact.
    assert!(root.path().join("daemon.sock").exists());
    std::fs::remove_file(root.path().join("daemon.sock")).unwrap();
    let mut replacement_daemon = fixture(root.path()).await;
    let replacement = connect(root.path()).await.unwrap();
    assert_ne!(replacement.instance_id, original);
    assert!(reconnected.current_guard().await.is_err());
    CodexRuntime::shutdown(root.path()).await;
    assert!(replacement_daemon.try_wait().unwrap().is_none());
    replacement_daemon.kill().await.unwrap();
}

#[tokio::test]
async fn missing_daemon_is_unavailable_without_creating_runtime_resources() {
    let root = tempfile::tempdir().unwrap();
    assert!(matches!(
        connect(root.path()).await,
        Err(Error::CapabilityUnavailable(_))
    ));
    assert!(!root.path().join("daemon.sock").exists());
    assert!(!root.path().join("state").exists());
    assert!(!registry().lock().await.contains_key(root.path()));
}

#[tokio::test]
async fn unavailable_daemon_recovers_unbound_session_without_claiming_ready() {
    let root = tempfile::tempdir().unwrap();
    let mut daemon = fixture(root.path()).await;
    let runtime = connect(root.path()).await.unwrap();
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
    let session = created.session_id().unwrap();
    let mut disconnected = runtime.connection.events.subscribe();
    daemon.kill().await.unwrap();
    tokio::time::timeout(Duration::from_secs(2), disconnected.recv())
        .await
        .unwrap()
        .unwrap();
    let (stop, receiver) = tokio::sync::watch::channel(false);
    let observer = tokio::spawn(
        crate::CodexObserver::new(app.event_ingest_service(), root.path().into()).run(receiver),
    );
    for expected in ["unavailable", "awaiting_input"] {
        if expected == "awaiting_input" {
            std::fs::remove_file(root.path().join("daemon.sock")).unwrap();
            daemon = fixture(root.path()).await;
        }
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let state: String = sqlx::query_scalar("SELECT json_extract(adapter_details,'$.codex.connection') FROM runtime_bindings WHERE session_id=?")
                    .bind(session).fetch_one(&app.db()).await.unwrap();
                if state == expected { break; }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }).await.unwrap();
        assert_eq!(
            app.event_ingest_service()
                .get_session(session)
                .await
                .unwrap()
                .unwrap()
                .state
                .to_string(),
            "created"
        );
        assert!(
            pontia_application::AgentBindingService::new(app.db())
                .binding_for_session(session)
                .await
                .unwrap()
                .is_none()
        );
    }
    stop.send(true).unwrap();
    observer.await.unwrap();
    CodexRuntime::shutdown(root.path()).await;
    assert!(daemon.try_wait().unwrap().is_none());
    daemon.kill().await.unwrap();
}

#[tokio::test]
async fn same_daemon_reconnection_requires_thread_reconciliation_before_control() {
    let root = tempfile::tempdir().unwrap();
    let mut daemon = fixture(root.path()).await;
    let original = connect(root.path()).await.unwrap();
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
    let session = created.session_id().unwrap();
    let service = crate::CodexService::new(app.event_ingest_service());
    let thread = json!({"id":"thread","cwd":root.path(),"canAcceptDirectInput":true,"status":{"type":"idle"}});
    service
        .bind(session, &original, &thread, None)
        .await
        .unwrap();
    pontia_application::sessions::NativeSessionService::new(app.db(), app.event_ingest_service())
        .ready(
            session,
            &original.instance_id,
            json!({"client_session_key":"thread","launch_cwd":root.path()}),
        )
        .await
        .unwrap();
    service
        .connection_state(session, &original, "available")
        .await
        .unwrap();
    let target = pontia_application::runtime::ControlTarget::resolve(
        &app.db(),
        session,
        Some(&original.instance_id),
    )
    .await
    .unwrap();
    service
        .confirm_control_connection(&target, &original)
        .await
        .unwrap();
    original.connection.close().await;
    let replacement = connect(root.path()).await.unwrap();
    assert_eq!(original.instance_id, replacement.instance_id);
    assert_ne!(original.connection_id, replacement.connection_id);
    // A saved available flag belongs to the old connection, not the daemon as a whole.
    for outcome in [
        service.set_model(&target, "model").await,
        service.interrupt(&target, "turn").await,
        service.archive(&target).await,
    ] {
        assert!(
            matches!(outcome, Err(Error::CapabilityUnavailable(_))),
            "{outcome:?}"
        );
    }
    service
        .bind(session, &replacement, &thread, Some(&original.instance_id))
        .await
        .unwrap();

    service
        .connection_state(session, &replacement, "available")
        .await
        .unwrap();
    service
        .confirm_control_connection(&target, &replacement)
        .await
        .unwrap();
    // A late availability update from the old connection cannot overwrite recovery.
    assert!(
        service
            .connection_state(session, &original, "unavailable")
            .await
            .is_err()
    );
    service
        .confirm_control_connection(&target, &replacement)
        .await
        .unwrap();
    CodexRuntime::shutdown(root.path()).await;
    daemon.kill().await.unwrap();
}
