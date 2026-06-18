use pontia_storage_sqlite::{
    connect_sqlite,
    repositories::agent_bindings::{AgentBindingUpsertRecord, SqliteAgentBindingRepository},
    run_migrations,
};
use serde_json::json;

async fn test_pool() -> sqlx::SqlitePool {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("sqlite_agent_binding_repository.db");
    let _kept_dir = dir.keep();
    let database_url = format!("sqlite://{}", db_path.display());
    let pool = connect_sqlite(&database_url).await.expect("connect");
    run_migrations(&pool).await.expect("migrate");
    pool
}

async fn insert_session(pool: &sqlx::SqlitePool, session_id: &str) {
    sqlx::query("INSERT INTO sessions (session_id, client_type, state, metadata) VALUES (?, 'pi', 'ready', '{}')")
        .bind(session_id)
        .execute(pool)
        .await
        .expect("insert session");
}

#[tokio::test]
async fn upserts_agent_binding_and_preserves_discovered_flag() {
    let pool = test_pool().await;
    insert_session(&pool, "sess_agent_binding").await;
    let repository = SqliteAgentBindingRepository::new(pool);

    let inserted = repository
        .upsert_binding(AgentBindingUpsertRecord {
            id: "bind_first".to_string(),
            session_id: "sess_agent_binding".to_string(),
            client_type: "pi".to_string(),
            launch_cwd: "/workspace/one".to_string(),
            client_session_key: "client-key".to_string(),
            metadata: json!({"version": 1}).to_string(),
        })
        .await
        .expect("insert binding");
    repository
        .mark_discovered(&inserted.id)
        .await
        .expect("mark discovered");

    let updated = repository
        .upsert_binding(AgentBindingUpsertRecord {
            id: "bind_second".to_string(),
            session_id: "sess_agent_binding".to_string(),
            client_type: "pi".to_string(),
            launch_cwd: "/workspace/two".to_string(),
            client_session_key: "client-key".to_string(),
            metadata: json!({"version": 2}).to_string(),
        })
        .await
        .expect("update binding");

    assert_eq!(updated.id, inserted.id);
    assert_eq!(updated.launch_cwd, "/workspace/one");
    assert_eq!(updated.metadata, json!({"version": 2}).to_string());
    assert!(updated.discovered);
}

#[tokio::test]
async fn primary_binding_for_session_returns_earliest_binding() {
    let pool = test_pool().await;
    insert_session(&pool, "sess_primary").await;
    let repository = SqliteAgentBindingRepository::new(pool);

    repository
        .upsert_binding(AgentBindingUpsertRecord {
            id: "bind_old".to_string(),
            session_id: "sess_primary".to_string(),
            client_type: "pi".to_string(),
            launch_cwd: "/workspace".to_string(),
            client_session_key: "old-key".to_string(),
            metadata: "{}".to_string(),
        })
        .await
        .expect("insert old binding");
    repository
        .upsert_binding(AgentBindingUpsertRecord {
            id: "bind_new".to_string(),
            session_id: "sess_primary".to_string(),
            client_type: "pi".to_string(),
            launch_cwd: "/workspace".to_string(),
            client_session_key: "new-key".to_string(),
            metadata: "{}".to_string(),
        })
        .await
        .expect("insert new binding");

    let primary = repository
        .primary_binding_for_session("sess_primary")
        .await
        .expect("primary binding")
        .expect("binding exists");

    assert_eq!(primary.id, "bind_old");
    assert_eq!(primary.client_session_key, "old-key");
}

#[tokio::test]
async fn looks_up_session_and_latest_key_from_agent_bindings() {
    let pool = test_pool().await;
    insert_session(&pool, "sess_lookup").await;
    let repository = SqliteAgentBindingRepository::new(pool);

    repository
        .upsert_binding(AgentBindingUpsertRecord {
            id: "bind_lookup_old".to_string(),
            session_id: "sess_lookup".to_string(),
            client_type: "pi".to_string(),
            launch_cwd: "/workspace".to_string(),
            client_session_key: "old-key".to_string(),
            metadata: "{}".to_string(),
        })
        .await
        .expect("insert old binding");
    repository
        .upsert_binding(AgentBindingUpsertRecord {
            id: "bind_lookup_new".to_string(),
            session_id: "sess_lookup".to_string(),
            client_type: "pi".to_string(),
            launch_cwd: "/workspace".to_string(),
            client_session_key: "new-key".to_string(),
            metadata: "{}".to_string(),
        })
        .await
        .expect("insert new binding");

    assert_eq!(
        repository
            .session_id_for_client_session("pi", "new-key")
            .await
            .expect("session lookup"),
        Some("sess_lookup".to_string())
    );
    assert_eq!(
        repository
            .latest_client_session_key("sess_lookup", "pi")
            .await
            .expect("latest key"),
        Some("new-key".to_string())
    );
}

#[tokio::test]
async fn agent_binding_can_be_upserted_inside_transaction() {
    let pool = test_pool().await;
    insert_session(&pool, "sess_tx_agent_binding").await;

    let mut tx = pool.begin().await.expect("begin tx");
    let binding = SqliteAgentBindingRepository::upsert_binding_in_tx(
        &mut tx,
        AgentBindingUpsertRecord {
            id: "bind_tx".to_string(),
            session_id: "sess_tx_agent_binding".to_string(),
            client_type: "pi".to_string(),
            launch_cwd: "/workspace/tx".to_string(),
            client_session_key: "tx-key".to_string(),
            metadata: json!({"tx": true}).to_string(),
        },
    )
    .await
    .expect("upsert in tx");
    tx.commit().await.expect("commit tx");

    let repository = SqliteAgentBindingRepository::new(pool);
    let primary = repository
        .primary_binding_for_session("sess_tx_agent_binding")
        .await
        .expect("primary binding")
        .expect("binding exists");

    assert_eq!(primary, binding);
}
