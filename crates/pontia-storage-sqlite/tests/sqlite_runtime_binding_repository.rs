use pontia_storage_sqlite::{
    connect_sqlite,
    repositories::session_runtimes::{SessionRuntimeRecord, SqliteSessionRuntimeRepository},
    run_migrations,
};

async fn test_pool() -> (sqlx::SqlitePool, tempfile::TempDir) {
    let root = tempfile::tempdir().unwrap();
    let pool = connect_sqlite(&format!(
        "sqlite://{}",
        root.path().join("runtimes.db").display()
    ))
    .await
    .unwrap();
    run_migrations(&pool).await.unwrap();
    sqlx::query("INSERT INTO sessions(session_id, client_type, state) VALUES ('session', 'pi', 'idle'), ('other', 'pi', 'idle')").execute(&pool).await.unwrap();
    (pool, root)
}

fn runtime(id: &str) -> SessionRuntimeRecord {
    SessionRuntimeRecord {
        runtime_id: id.into(),
        session_id: "session".into(),
        role: "tui".into(),
        state: "running".into(),
        start_command: Some("pi --approve".into()),
        tmux_socket_path: Some("/tmp/test.sock".into()),
        tmux_pane_id: Some("%1".into()),
        process_fingerprint: None,
        created_at: "2026-10-01T00:00:00Z".into(),
    }
}

#[tokio::test]
async fn updating_one_runtime_preserves_other_runtimes_and_creation_time() {
    let (pool, _root) = test_pool().await;
    let repository = SqliteSessionRuntimeRepository::new(pool);
    let a = runtime("a");
    let b = runtime("b");
    repository.upsert_binding(a.clone()).await.unwrap();
    repository.upsert_binding(b.clone()).await.unwrap();
    let mut exited = a.clone();
    exited.state = "exited".into();
    repository.upsert_binding(exited.clone()).await.unwrap();
    let mut restarting = exited;
    restarting.state = "starting".into();
    restarting.created_at = "2099-01-01T00:00:00Z".into();
    repository.upsert_binding(restarting).await.unwrap();
    let mut running = a.clone();
    running.tmux_pane_id = Some("%9".into());
    repository.upsert_binding(running.clone()).await.unwrap();
    assert_eq!(repository.get("a").await.unwrap(), Some(running));
    assert_eq!(repository.get("b").await.unwrap(), Some(b));
    assert_eq!(repository.list("session").await.unwrap().len(), 2);
    assert!(
        repository.runtime_id("session").await.is_err(),
        "ambiguous control must not choose an arbitrary TUI"
    );
}

#[tokio::test]
async fn runtime_cannot_move_to_another_session() {
    let (pool, _root) = test_pool().await;
    let repository = SqliteSessionRuntimeRepository::new(pool);
    let original = runtime("a");
    repository.upsert_binding(original.clone()).await.unwrap();
    let mut moved = original.clone();
    moved.session_id = "other".into();
    assert!(repository.upsert_binding(moved).await.is_err());
    assert_eq!(repository.get("a").await.unwrap(), Some(original));
    assert!(repository.list("other").await.unwrap().is_empty());
}

#[tokio::test]
async fn rejects_invalid_runtime_lifecycle_state() {
    let (pool, _root) = test_pool().await;
    let repository = SqliteSessionRuntimeRepository::new(pool);
    let mut invalid = runtime("invalid");
    invalid.state = "busy".into();

    assert!(repository.upsert_binding(invalid).await.is_err());
    assert!(repository.list("session").await.unwrap().is_empty());
}

#[tokio::test]
async fn rejects_a_partial_tmux_location() {
    let (pool, _root) = test_pool().await;
    let repository = SqliteSessionRuntimeRepository::new(pool);
    let mut invalid = runtime("invalid");
    invalid.tmux_pane_id = None;

    assert!(repository.upsert_binding(invalid).await.is_err());
    assert!(repository.list("session").await.unwrap().is_empty());
}

#[tokio::test]
async fn stores_fingerprint_text_without_database_json_validation() {
    let (pool, _root) = test_pool().await;
    let repository = SqliteSessionRuntimeRepository::new(pool);
    let mut fingerprinted = runtime("fingerprinted");
    fingerprinted.process_fingerprint = Some("{\"agent_pid\":42}".into());

    repository
        .upsert_binding(fingerprinted.clone())
        .await
        .unwrap();
    assert_eq!(
        repository.get("fingerprinted").await.unwrap(),
        Some(fingerprinted)
    );
}
