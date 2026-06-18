use pontia_storage_sqlite::{
    connect_sqlite,
    repositories::runtime_bindings::{RuntimeBindingUpsertRecord, SqliteRuntimeBindingRepository},
    run_migrations,
};
use serde_json::json;

async fn test_pool() -> sqlx::SqlitePool {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("sqlite_runtime_binding_repository.db");
    let _kept_dir = dir.keep();
    let database_url = format!("sqlite://{}", db_path.display());
    let pool = connect_sqlite(&database_url).await.expect("connect");
    run_migrations(&pool).await.expect("migrate");
    pool
}

#[tokio::test]
async fn upserts_runtime_binding_and_replaces_structured_fields() {
    let pool = test_pool().await;
    sqlx::query("INSERT INTO sessions (session_id, client_type, state, metadata) VALUES ('sess_runtime', 'pi', 'ready', '{}')")
        .execute(&pool)
        .await
        .expect("insert session");
    let repository = SqliteRuntimeBindingRepository::new(pool);

    repository
        .upsert_binding(RuntimeBindingUpsertRecord {
            session_id: "sess_runtime".to_string(),
            runtime_kind: "tmux".to_string(),
            runtime_instance_id: Some("rtinst_one".to_string()),
            start_command: Some("pi --resume".to_string()),
            launch_cwd: Some("/workspace/one".to_string()),
            last_seen_at: Some("2026-06-18T12:00:00Z".to_string()),
            tmux_socket_path: Some("/tmp/tmux.sock".to_string()),
            tmux_pane_id: Some("%1".to_string()),
            metadata: json!({"restart_count": 1}).to_string(),
        })
        .await
        .expect("insert binding");
    repository
        .upsert_binding(RuntimeBindingUpsertRecord {
            session_id: "sess_runtime".to_string(),
            runtime_kind: "tmux".to_string(),
            runtime_instance_id: Some("rtinst_two".to_string()),
            start_command: Some("pi --again".to_string()),
            launch_cwd: Some("/workspace/two".to_string()),
            last_seen_at: Some("2026-06-18T12:01:00Z".to_string()),
            tmux_socket_path: Some("/tmp/tmux2.sock".to_string()),
            tmux_pane_id: Some("%2".to_string()),
            metadata: json!({"restart_count": 2}).to_string(),
        })
        .await
        .expect("update binding");

    assert_eq!(
        repository
            .start_command("sess_runtime")
            .await
            .expect("start command"),
        Some("pi --again".to_string())
    );
    assert_eq!(
        repository.metadata("sess_runtime").await.expect("metadata"),
        Some(json!({"restart_count": 2}).to_string())
    );
    let pane = repository
        .tmux_pane_binding("sess_runtime")
        .await
        .expect("pane binding")
        .expect("pane exists");
    assert_eq!(pane.socket_path.as_deref(), Some("/tmp/tmux2.sock"));
    assert_eq!(pane.pane_id.as_deref(), Some("%2"));
}

#[tokio::test]
async fn runtime_binding_metadata_can_be_read_inside_transaction() {
    let pool = test_pool().await;
    sqlx::query("INSERT INTO sessions (session_id, client_type, state, metadata) VALUES ('sess_tx', 'pi', 'ready', '{}')")
        .execute(&pool)
        .await
        .expect("insert session");
    let repository = SqliteRuntimeBindingRepository::new(pool.clone());
    repository
        .upsert_binding(RuntimeBindingUpsertRecord {
            session_id: "sess_tx".to_string(),
            runtime_kind: "tmux".to_string(),
            runtime_instance_id: Some("rtinst_tx".to_string()),
            start_command: None,
            launch_cwd: Some("/workspace/tx".to_string()),
            last_seen_at: None,
            tmux_socket_path: None,
            tmux_pane_id: None,
            metadata: json!({"workspace": "/workspace/tx"}).to_string(),
        })
        .await
        .expect("insert binding");

    let mut tx = pool.begin().await.expect("begin tx");
    assert_eq!(
        SqliteRuntimeBindingRepository::metadata_in_tx(&mut tx, "sess_tx")
            .await
            .expect("metadata in tx"),
        Some(json!({"workspace": "/workspace/tx"}).to_string())
    );
    tx.rollback().await.expect("rollback tx");
}
