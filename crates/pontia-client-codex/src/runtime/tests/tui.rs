use super::*;
use std::os::unix::fs::PermissionsExt;

pub(super) fn launcher(root: &Path) -> String {
    let path = root.join("tui-fixture");
    let executable = std::env::current_exe().unwrap();
    // --skip consumes the literal --remote; the following endpoint/thread are
    // extra test filters. The child still exposes the actual launch identity.
    std::fs::write(&path, format!("#!/bin/sh\nexec '{}' --ignored --exact runtime::tests::tui::tui_peer_fixture --nocapture --skip \"$2\" \"$3\" \"$4\"\n", executable.display())).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
    path.to_string_lossy().into_owned()
}

pub(super) struct TuiCleanup(pub PathBuf);
impl Drop for TuiCleanup {
    fn drop(&mut self) {
        let _ = std::process::Command::new("tmux")
            .args([
                "-S",
                &self.0.join("state/codex/tmux.sock").to_string_lossy(),
                "kill-server",
            ])
            .output();
    }
}

#[tokio::test]
#[ignore = "owned TUI subprocess fixture"]
async fn tui_peer_fixture() {
    let args: Vec<_> = std::env::args().collect();
    let Some(index) = args.iter().position(|arg| arg == "--remote") else {
        return;
    };
    let socket = Path::new(args[index + 1].trim_start_matches("unix://"));
    let thread = &args[index + 2];
    let mut connection = Connection::connect(socket).await.unwrap();
    connection
        .call(
            "thread/resume",
            json!({"threadId":thread,"excludeTurns":true}),
        )
        .await
        .ok();
    let commands = socket.with_extension("command");
    let mut uninitialized = None;
    loop {
        if let Ok(command) = std::fs::read_to_string(&commands) {
            if command.is_empty() {
                continue;
            }
            std::fs::remove_file(&commands).unwrap();
            if command == "disconnect" {
                connection.close().await;
            } else if command == "resume-pending" {
                connection
                    .call("thread/resume", json!({"threadId":thread,"testHold":true}))
                    .await
                    .ok();
            } else if command == "takeover" {
                uninitialized = Some(super::super::protocol::open(socket).await.unwrap());
            } else if command == "reconnect" {
                uninitialized.take();
                connection = Connection::connect(socket).await.unwrap();
                connection
                    .call(
                        "thread/resume",
                        json!({"threadId":thread,"excludeTurns":true}),
                    )
                    .await
                    .unwrap();
            } else {
                connection
                    .call(
                        "thread/resume",
                        json!({"threadId":command,"excludeTurns":true}),
                    )
                    .await
                    .ok();
            }
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

async fn expire_reconnect(runtime: &CodexRuntime, owner: &str) {
    runtime
        .gateways
        .lock()
        .await
        .get(owner)
        .unwrap()
        .state
        .lock()
        .await
        .quiet_since = Some(tokio::time::Instant::now() - super::super::tui::RECONNECT_WINDOW);
}

async fn wait_target(runtime: &CodexRuntime, owner: &str, thread: &str, connected: bool) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if runtime.saved_target(owner).unwrap().is_some_and(|target| {
                target.thread["id"] == thread && target.connected == connected
            }) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
}

async fn connect_fixture(root: &Path) -> Arc<CodexRuntime> {
    let mut runtime = lifecycle::connect(root).await.unwrap();
    registry().lock().await.remove(&runtime.root);
    Arc::get_mut(&mut runtime).unwrap().tui_command = launcher(root);
    registry()
        .lock()
        .await
        .insert(runtime.root.clone(), runtime.clone());
    runtime
}

pub(super) async fn session(app: &AppState, runtime: &CodexRuntime, thread: &str) -> String {
    let created = app
        .session_commands()
        .create_session(
            serde_json::from_value(json!({"client_type":"codex","workspace":runtime.root}))
                .unwrap(),
        )
        .await
        .unwrap();
    let id = created.session_id().unwrap().to_string();
    crate::CodexService::new(app.event_ingest_service())
        .bind(
            &id,
            runtime,
            &json!({"id":thread,"cwd":runtime.root,"canAcceptDirectInput":true}),
            None,
        )
        .await
        .unwrap();
    id
}

pub(super) async fn app(root: &Path) -> AppState {
    let pool = pontia_storage_sqlite::connect_sqlite(&format!(
        "sqlite://{}",
        root.join("test.db").display()
    ))
    .await
    .unwrap();
    pontia_storage_sqlite::run_migrations(&pool).await.unwrap();
    let mut clients = ClientRegistry::default();
    clients.register(crate::registration());
    AppState::builder(pool, root.into())
        .clients(clients)
        .build()
}

#[tokio::test]
async fn open_tui_recovers_only_the_confirmed_owner_after_restart() {
    let root = tempfile::tempdir().unwrap();
    let _tuis = TuiCleanup(root.path().into());
    let mut daemon = lifecycle::fixture(root.path()).await;
    let runtime = connect_fixture(root.path()).await;
    let app = app(root.path()).await;
    let first = session(&app, &runtime, "first").await;
    let second = session(&app, &runtime, "second").await;
    let switched = session(&app, &runtime, "switched").await;
    let service = crate::CodexService::new(app.event_ingest_service());
    let (a, b) = tokio::join!(service.open_tui(&first), service.open_tui(&first));
    a.unwrap();
    b.unwrap();
    service.open_tui(&second).await.unwrap();
    let original: super::super::tui::TuiProcess =
        runtime.saved_tui(&first, "process").unwrap().unwrap();
    let other: super::super::tui::TuiProcess =
        runtime.saved_tui(&second, "process").unwrap().unwrap();
    assert!(original.is_alive() && other.is_alive());
    let endpoint = runtime.gateway(&first).await.unwrap();
    let commands = Path::new(endpoint.trim_start_matches("unix://")).with_extension("command");
    std::fs::write(&commands, "takeover").unwrap();
    wait_target(&runtime, &first, "first", false).await;
    assert!(
        service.open_tui(&first).await.is_err(),
        "a new connection must confirm its own attachment"
    );
    assert!(original.is_alive());
    std::fs::write(&commands, "reconnect").unwrap();
    wait_target(&runtime, &first, "first", true).await;
    std::fs::write(&commands, "disconnect").unwrap();
    wait_target(&runtime, &first, "first", false).await;
    assert!(service.open_tui(&first).await.is_err());
    assert!(original.is_alive());
    std::fs::write(&commands, "reconnect").unwrap();
    wait_target(&runtime, &first, "first", true).await;
    service.open_tui(&first).await.unwrap();
    assert_eq!(
        runtime.tui_pane(&first).unwrap(),
        (original.socket.clone(), original.pane.clone())
    );
    std::fs::write(
        Path::new(endpoint.trim_start_matches("unix://")).with_extension("command"),
        "switched",
    )
    .unwrap();
    wait_target(&runtime, &first, "switched", true).await;
    assert!(matches!(
        service.open_tui(&first).await,
        Err(Error::StateConflict(_))
    ));
    service.open_tui(&switched).await.unwrap();
    assert_eq!(
        runtime.tui_pane(&first).unwrap(),
        (original.socket.clone(), original.pane.clone())
    );
    // Stop before the observer has persisted the switched target to the database.
    sqlx::query(
        "UPDATE codex_tui_bindings SET target_session_id=?,connected=TRUE WHERE owner_session_id=?",
    )
    .bind(&first)
    .bind(&first)
    .execute(&app.db())
    .await
    .unwrap();
    let direct = Connection::connect(&runtime.socket_path).await.unwrap();
    CodexRuntime::shutdown(root.path()).await;
    let replacement = connect_fixture(root.path()).await;
    assert!(
        service.open_tui(&switched).await.is_err(),
        "native recovery window must preserve the old process"
    );
    assert!(original.is_alive());
    expire_reconnect(&replacement, &first).await;
    let (a, b) = tokio::join!(service.open_tui(&switched), service.open_tui(&switched));
    a.unwrap();
    b.unwrap();
    let recovered: super::super::tui::TuiProcess =
        replacement.saved_tui(&first, "process").unwrap().unwrap();
    assert_ne!(recovered.peer_identity(), original.peer_identity());
    assert!(!original.is_alive());
    assert!(recovered.is_alive() && other.is_alive());
    assert!(
        replacement
            .saved_tui::<super::super::tui::TuiProcess>(&switched, "process")
            .unwrap()
            .is_none()
    );
    assert_eq!(
        replacement.saved_target(&first).unwrap().unwrap().thread["id"],
        "switched"
    );
    assert!(daemon.try_wait().unwrap().is_none());
    assert!(root.path().join("daemon.sock").exists());
    direct
        .call("thread/read", json!({"threadId":"second"}))
        .await
        .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM turns")
            .fetch_one(&app.db())
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM sessions WHERE state='exited'")
            .fetch_one(&app.db())
            .await
            .unwrap(),
        0
    );
    CodexRuntime::shutdown(root.path()).await;
    daemon.kill().await.unwrap();
}

#[tokio::test]
async fn open_tui_refuses_uncertain_targets_and_reused_panes() {
    let root = tempfile::tempdir().unwrap();
    let _tuis = TuiCleanup(root.path().into());
    let mut daemon = lifecycle::fixture(root.path()).await;
    let runtime = connect_fixture(root.path()).await;
    let app = app(root.path()).await;
    let first = session(&app, &runtime, "first").await;
    let service = crate::CodexService::new(app.event_ingest_service());
    service.open_tui(&first).await.unwrap();
    let process: super::super::tui::TuiProcess =
        runtime.saved_tui(&first, "process").unwrap().unwrap();
    let endpoint = runtime.gateway(&first).await.unwrap();
    let socket = Path::new(endpoint.trim_start_matches("unix://"));
    assert!(
        Connection::connect(socket).await.is_err(),
        "another process cannot take over an owned gateway"
    );
    std::fs::write(socket.with_extension("command"), "pending").unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while runtime.saved_target(&first).unwrap().unwrap().thread != Value::Null {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    CodexRuntime::shutdown(root.path()).await;
    let replacement = connect_fixture(root.path()).await;
    replacement.gateway(&first).await.unwrap();
    expire_reconnect(&replacement, &first).await;
    assert!(matches!(
        service.open_tui(&first).await,
        Err(Error::StateConflict(_))
    ));
    assert!(
        process.is_alive(),
        "an unconfirmed target never permits process replacement"
    );
    assert!(
        std::process::Command::new("tmux")
            .args([
                "-S",
                &process.socket,
                "respawn-pane",
                "-k",
                "-t",
                &process.pane,
                "sleep 60"
            ])
            .status()
            .unwrap()
            .success()
    );
    assert!(matches!(
        service.open_tui(&first).await,
        Err(Error::StateConflict(_))
    ));
    assert!(
        pontia_runtime::GenericRuntimeManager.is_tmux_pane_alive(&process.socket, &process.pane)
    );
    CodexRuntime::shutdown(root.path()).await;
    daemon.kill().await.unwrap();
}

#[tokio::test]
async fn attachment_rejection_is_not_reported_as_open_success() {
    let root = tempfile::tempdir().unwrap();
    let _tuis = TuiCleanup(root.path().into());
    let mut daemon = lifecycle::fixture(root.path()).await;
    let runtime = connect_fixture(root.path()).await;
    let app = app(root.path()).await;
    let rejected = session(&app, &runtime, "rejected").await;
    let error = crate::CodexService::new(app.event_ingest_service())
        .open_tui(&rejected)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("thread unavailable"), "{error}");
    assert!(
        !sqlx::query_scalar::<_, bool>(
            "SELECT connected FROM codex_tui_bindings WHERE owner_session_id=?"
        )
        .bind(&rejected)
        .fetch_one(&app.db())
        .await
        .unwrap()
    );
    CodexRuntime::shutdown(root.path()).await;
    daemon.kill().await.unwrap();
}

#[tokio::test]
async fn rejected_switch_does_not_block_reuse_or_recovery_of_the_original_thread() {
    let root = tempfile::tempdir().unwrap();
    let _tuis = TuiCleanup(root.path().into());
    let mut daemon = lifecycle::fixture(root.path()).await;
    let runtime = connect_fixture(root.path()).await;
    let app = app(root.path()).await;
    let first = session(&app, &runtime, "first").await;
    let service = crate::CodexService::new(app.event_ingest_service());
    service.open_tui(&first).await.unwrap();
    let original: super::super::tui::TuiProcess =
        runtime.saved_tui(&first, "process").unwrap().unwrap();
    let endpoint = runtime.gateway(&first).await.unwrap();
    let command = Path::new(endpoint.trim_start_matches("unix://")).with_extension("command");
    std::fs::write(&command, "rejected").unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while runtime
            .saved_target(&first)
            .unwrap()
            .unwrap()
            .error
            .is_none()
        {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    service.open_tui(&first).await.unwrap();
    assert!(original.is_alive());
    std::fs::write(&command, "disconnect").unwrap();
    wait_target(&runtime, &first, "first", false).await;
    expire_reconnect(&runtime, &first).await;
    service.open_tui(&first).await.unwrap();
    assert!(!original.is_alive());
    assert_eq!(
        runtime.saved_target(&first).unwrap().unwrap().thread["id"],
        "first"
    );
    CodexRuntime::shutdown(root.path()).await;
    daemon.kill().await.unwrap();
}

#[tokio::test]
async fn interrupted_resume_of_the_same_thread_preserves_the_recovery_target() {
    let root = tempfile::tempdir().unwrap();
    let _tuis = TuiCleanup(root.path().into());
    let mut daemon = lifecycle::fixture(root.path()).await;
    let runtime = connect_fixture(root.path()).await;
    let app = app(root.path()).await;
    let first = session(&app, &runtime, "first").await;
    let service = crate::CodexService::new(app.event_ingest_service());
    service.open_tui(&first).await.unwrap();
    let original: super::super::tui::TuiProcess =
        runtime.saved_tui(&first, "process").unwrap().unwrap();
    let endpoint = runtime.gateway(&first).await.unwrap();
    std::fs::write(
        Path::new(endpoint.trim_start_matches("unix://")).with_extension("command"),
        "resume-pending",
    )
    .unwrap();
    wait_target(&runtime, &first, "first", false).await;
    CodexRuntime::shutdown(root.path()).await;
    let replacement = connect_fixture(root.path()).await;
    replacement.gateway(&first).await.unwrap();
    expire_reconnect(&replacement, &first).await;
    service.open_tui(&first).await.unwrap();
    assert!(!original.is_alive());
    assert_eq!(
        replacement.saved_target(&first).unwrap().unwrap().thread["id"],
        "first"
    );
    CodexRuntime::shutdown(root.path()).await;
    daemon.kill().await.unwrap();
}
