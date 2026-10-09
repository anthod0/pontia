use pontia_application::{AppState, client_contract::ClientServicePhase, clients::ClientRegistry};
use pontia_core::Error;

#[tokio::test]
async fn registered_service_owns_the_listener_until_shutdown() {
    let root = tempfile::tempdir().unwrap();
    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    let mut clients = ClientRegistry::default();
    clients.register(pontia_client_pi::registration(None));
    let state = AppState::builder(pool, root.path().to_path_buf())
        .clients(clients.clone())
        .build();

    let tasks = clients
        .start_services(state.clone(), ClientServicePhase::Transport)
        .await
        .unwrap();
    let path = root.path().join("state/pi/rpc.sock");
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
