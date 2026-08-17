use pontia_storage_sqlite::{
    connect_sqlite,
    repositories::runtime_bindings::{
        PendingTurnContextRecord, RuntimeBindingConfirmationRecord, RuntimeBindingUpsertRecord,
        SqliteRuntimeBindingRepository,
    },
    run_migrations,
};

async fn test_pool() -> (sqlx::SqlitePool, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("sqlite_runtime_binding_repository.db");
    let database_url = format!("sqlite://{}", db_path.display());
    let pool = connect_sqlite(&database_url).await.expect("connect");
    run_migrations(&pool).await.expect("migrate");
    (pool, dir)
}

fn binding(runtime_instance_id: &str, suffix: &str) -> RuntimeBindingUpsertRecord {
    RuntimeBindingUpsertRecord {
        session_id: "sess_runtime".to_string(),
        runtime_kind: "tmux".to_string(),
        runtime_instance_id: Some(runtime_instance_id.to_string()),
        binding_state: "provisioned".to_string(),
        runtime_handle: Some(format!("runtime-{suffix}")),
        start_command: Some(format!("pi --{suffix}")),
        launch_cwd: Some(format!("/workspace/{suffix}")),
        internal_event_url: Some("http://127.0.0.1/internal/v1/events".to_string()),
        started_at: Some("2026-06-18T12:00:00Z".to_string()),
        last_seen_at: Some("2026-06-18T12:01:00Z".to_string()),
        restart_count: 1,
        tmux_socket_path: Some(format!("/tmp/{suffix}.sock")),
        tmux_pane_id: Some(format!("%{suffix}")),
        process_fingerprint: None,
        capabilities: "{}".to_string(),
        diagnostics: format!(r#"{{"runtime_log":"/{suffix}.log"}}"#),
        adapter_details: "{}".to_string(),
    }
}

#[tokio::test]
async fn upserts_runtime_binding_and_replaces_structured_fields() {
    let (pool, _pontia_home) = test_pool().await;
    sqlx::query("INSERT INTO sessions (session_id, client_type, state, metadata) VALUES ('sess_runtime', 'pi', 'ready', '{}')")
        .execute(&pool)
        .await
        .expect("insert session");
    let repository = SqliteRuntimeBindingRepository::new(pool);

    repository
        .upsert_binding(binding("rtinst_one", "one"))
        .await
        .expect("insert binding");
    repository
        .upsert_binding(binding("rtinst_two", "two"))
        .await
        .expect("update binding");

    assert_eq!(
        repository
            .start_command("sess_runtime")
            .await
            .expect("start command"),
        Some("pi --two".to_string())
    );
    assert_eq!(
        repository
            .runtime_handle("sess_runtime")
            .await
            .expect("runtime handle"),
        Some("runtime-two".to_string())
    );
    let pane = repository
        .tmux_pane_binding("sess_runtime")
        .await
        .expect("pane binding")
        .expect("pane exists");
    assert_eq!(pane.runtime_instance_id.as_deref(), Some("rtinst_two"));
    assert_eq!(pane.socket_path.as_deref(), Some("/tmp/two.sock"));
    assert_eq!(pane.pane_id.as_deref(), Some("%two"));
}

#[tokio::test]
async fn stale_provisioning_write_cannot_downgrade_confirmed_process_identity() {
    let (pool, _pontia_home) = test_pool().await;
    sqlx::query("INSERT INTO sessions (session_id, client_type, state, metadata) VALUES ('sess_runtime', 'pi', 'starting', '{}')")
        .execute(&pool)
        .await
        .expect("insert session");
    let repository = SqliteRuntimeBindingRepository::new(pool.clone());
    let stale_provisioning = binding("rtinst_one", "one");
    repository
        .upsert_binding(stale_provisioning.clone())
        .await
        .expect("provision binding");

    let mut tx = pool.begin().await.expect("begin confirmation");
    SqliteRuntimeBindingRepository::confirm_binding_in_tx(
        &mut tx,
        RuntimeBindingConfirmationRecord {
            session_id: "sess_runtime".to_string(),
            runtime_kind: "tmux".to_string(),
            runtime_instance_id: "rtinst_one".to_string(),
            start_command: None,
            launch_cwd: "/workspace/one".to_string(),
            internal_event_url: "http://127.0.0.1/internal/v1/events".to_string(),
            last_seen_at: "2026-06-18T12:02:00Z".to_string(),
            tmux_socket_path: Some("/tmp/one.sock".to_string()),
            tmux_pane_id: Some("%one".to_string()),
            process_fingerprint: Some(r#"{"agent_pid":42}"#.to_string()),
            capabilities: "{}".to_string(),
            diagnostics: "{}".to_string(),
            adapter_details: "{}".to_string(),
        },
    )
    .await
    .expect("confirm binding");
    tx.commit().await.expect("commit confirmation");

    repository
        .upsert_binding(stale_provisioning)
        .await
        .expect("late provisioning write");

    let state_and_fingerprint: (String, Option<String>) = sqlx::query_as(
        "SELECT binding_state, process_fingerprint FROM runtime_bindings WHERE session_id = 'sess_runtime'",
    )
    .fetch_one(&pool)
    .await
    .expect("confirmed binding");
    assert_eq!(state_and_fingerprint.0, "confirmed");
    assert_eq!(
        state_and_fingerprint.1.as_deref(),
        Some(r#"{"agent_pid":42}"#)
    );
}

#[tokio::test]
async fn pending_turn_context_is_claimed_atomically_without_updating_runtime_binding() {
    let (pool, _pontia_home) = test_pool().await;
    sqlx::query("INSERT INTO sessions (session_id, client_type, state, metadata) VALUES ('sess_runtime', 'pi', 'ready', '{}')")
        .execute(&pool)
        .await
        .expect("insert session");
    let repository = SqliteRuntimeBindingRepository::new(pool.clone());
    repository
        .upsert_binding(binding("rtinst_one", "one"))
        .await
        .expect("insert binding");
    repository
        .store_pending_turn_context(PendingTurnContextRecord {
            session_id: "sess_runtime".to_string(),
            runtime_instance_id: "rtinst_one".to_string(),
            client_type: "pi".to_string(),
            payload: r#"{"input":"hello"}"#.to_string(),
        })
        .await
        .expect("store pending context");

    let before: (String, String) = sqlx::query_as(
        "SELECT binding_state, diagnostics FROM runtime_bindings WHERE session_id = 'sess_runtime'",
    )
    .fetch_one(&pool)
    .await
    .expect("binding before claim");
    assert_eq!(
        repository
            .claim_pending_turn_context("sess_runtime", "rtinst_one", "pi")
            .await
            .expect("claim"),
        Some(r#"{"input":"hello"}"#.to_string())
    );
    assert_eq!(
        repository
            .claim_pending_turn_context("sess_runtime", "rtinst_one", "pi")
            .await
            .expect("second claim"),
        None
    );
    let after: (String, String) = sqlx::query_as(
        "SELECT binding_state, diagnostics FROM runtime_bindings WHERE session_id = 'sess_runtime'",
    )
    .fetch_one(&pool)
    .await
    .expect("binding after claim");
    assert_eq!(after, before);
}
