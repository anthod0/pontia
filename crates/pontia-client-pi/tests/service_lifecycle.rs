mod support;
use pontia_agent_control::{
    AgentControl, client_contract::ClientServicePhase, clients::ClientRegistry,
};
use pontia_core::Error;

#[tokio::test]
async fn registered_service_owns_the_listener_until_shutdown() {
    let root = tempfile::tempdir().unwrap();
    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    let mut clients = ClientRegistry::default();
    clients.register(pontia_client_pi::registration());
    let state = AgentControl::builder(pool, root.path().to_path_buf())
        .clients(clients.clone())
        .build();

    let tasks = clients
        .start_services(state.clone(), ClientServicePhase::Transport)
        .await
        .unwrap();
    let path = pontia_agent_control::ipc::control_socket_path(root.path()).unwrap();
    assert!(tokio::net::UnixStream::connect(&path).await.is_ok());
    assert!(matches!(
        clients
            .start_services(state.clone(), ClientServicePhase::Transport)
            .await,
        Err(Error::StateConflict(_))
    ));

    state.shutdown().notify();
    tasks.join().await.unwrap();
    assert!(!path.exists());

    let restarted = clients
        .start_services(state, ClientServicePhase::Transport)
        .await
        .unwrap();
    restarted.join().await.unwrap();
    assert!(!path.exists());
}

#[tokio::test]
#[ignore = "requires installed pi and the clients/pi extension dependencies"]
async fn background_rpc_process_reconnects_and_confirms_idle_exit() {
    use pontia_agent_control::CreateSessionRequest;
    use pontia_runtime::{GenericRuntimeManager, ProcessFingerprint, ProcessObservation};
    use serde_json::json;
    use std::time::Duration;
    let root = tempfile::tempdir().unwrap();
    let pool = pontia_storage_sqlite::connect_sqlite("sqlite::memory:")
        .await
        .unwrap();
    pontia_storage_sqlite::run_migrations(&pool).await.unwrap();
    let extension = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../clients/pi/src/index.ts")
        .canonicalize()
        .unwrap();
    let mut clients = ClientRegistry::default();
    clients.register(pontia_client_pi::registration());
    let extensions = root.path().join("pi/extensions");
    std::fs::create_dir_all(&extensions).unwrap();
    std::os::unix::fs::symlink(extension, extensions.join("pontia.ts")).unwrap();
    pontia_storage_sqlite::repositories::agent_clients::SqliteAgentClientRepository::new(
        pool.clone(),
    )
    .save("pi", "default", true)
    .await
    .unwrap();
    pontia_agent_control::workspaces::upsert_canonical_workspace(
        &pool,
        root.path().to_str().unwrap(),
        None,
    )
    .await
    .unwrap();
    let build = || {
        AgentControl::builder(pool.clone(), root.path().into())
            .clients(clients.clone())
            .build()
    };
    let app = build();
    let tasks = clients
        .start_services(app.clone(), ClientServicePhase::Transport)
        .await
        .unwrap();
    let mut request: CreateSessionRequest =
        serde_json::from_value(json!({"client_type":"pi","workspace":root.path()})).unwrap();
    request.launch_mode = pontia_core::LaunchMode::Background;
    request.runtime_environment.insert(
        "PI_CODING_AGENT_DIR".into(),
        root.path().join("pi").display().to_string(),
    );
    let outcome = app
        .session_commands()
        .create_session(request)
        .await
        .unwrap();
    let session = outcome.session_id().unwrap();
    let (_runtime, fingerprint, pane, socket): (String, String, Option<String>, Option<String>) = sqlx::query_as(
        "SELECT runtime_id,process_fingerprint,tmux_pane_id,tmux_socket_path FROM session_runtimes WHERE session_id=?"
    ).bind(session).fetch_one(&pool).await.unwrap();
    assert_eq!((pane, socket), (None, None));
    struct Cleanup(ProcessFingerprint);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = GenericRuntimeManager.terminate_process(&self.0);
        }
    }
    let process = Cleanup(serde_json::from_str(&fingerprint).unwrap());
    async fn wait_state(app: &AgentControl, session: &str, state: &str) {
        tokio::time::timeout(Duration::from_secs(15), async {
            while app
                .queries()
                .session_snapshot(session)
                .await
                .unwrap()
                .unwrap()
                .state
                != state
            {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .unwrap_or_else(|_| {
            panic!(
                "waiting for {state}: {:?}, hook: {:?}",
                std::fs::read_to_string(app.pontia_home().join("state/runtime.log")),
                std::fs::read_to_string(app.pontia_home().join("state/pi-hook.log"))
            )
        });
    }
    wait_state(&app, session, "idle").await;
    app.shutdown().notify();
    tasks.join().await.unwrap();
    assert_eq!(process.0.observe(), ProcessObservation::Alive);
    let recovered = build();
    let tasks = clients
        .start_services(recovered.clone(), ClientServicePhase::Transport)
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(15), async {
        while !support::connected(&recovered, session).await {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap();
    let client = support::session(&recovered);
    let target = support::target(&recovered, session).await;
    assert_eq!(
        client
            .exit(
                recovered.event_ingest_service(),
                &target,
                pontia_agent_control::client_contract::ClientExitMode::IfIdle,
            )
            .await
            .unwrap(),
        pontia_agent_control::client_contract::ClientExitOutcome::Requested
    );
    wait_state(&recovered, session, "exited").await;
    recovered.shutdown().notify();
    tasks.join().await.unwrap();
}
