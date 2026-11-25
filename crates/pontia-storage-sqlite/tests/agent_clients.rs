use pontia_storage_sqlite::{
    connect_sqlite, repositories::agent_clients::SqliteAgentClientRepository, run_migrations,
};

#[tokio::test]
async fn fresh_and_existing_databases_enforce_client_identity_and_setting_constraints() {
    for existing in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let pool = connect_sqlite(&format!("sqlite://{}", root.path().join("db").display()))
            .await
            .unwrap();
        if existing {
            let previous = sqlx::migrate::Migrator {
                migrations: std::borrow::Cow::Owned(
                    sqlx::migrate!("./migrations")
                        .iter()
                        .filter(|migration| migration.version < 36)
                        .cloned()
                        .collect(),
                ),
                ..sqlx::migrate::Migrator::DEFAULT
            };
            previous.run(&pool).await.unwrap();
            sqlx::query("INSERT INTO sessions(session_id, client_type, state) VALUES ('existing','pi','idle')").execute(&pool).await.unwrap();
        }
        run_migrations(&pool).await.unwrap();
        let repository = SqliteAgentClientRepository::new(pool.clone());
        assert!(repository.list().await.unwrap().is_empty());
        for peer in ["grok", "company"] {
            repository.save("acp", peer, true).await.unwrap();
        }
        assert_eq!(repository.list().await.unwrap().len(), 2);
        let grok = repository.get("acp", "grok").await.unwrap().unwrap();
        assert!(grok.enabled);
        assert!(!grok.created_at.is_empty());
        assert_eq!(grok.created_at, grok.updated_at);
        assert!(
            sqlx::query("INSERT INTO agent_clients(client_type,client_key) VALUES ('acp','grok')")
                .execute(&pool)
                .await
                .is_err()
        );
        for invalid in ["-1", "2", "0.5", "'true'", "NULL"] {
            assert!(sqlx::query(&format!("INSERT INTO agent_clients(client_type,client_key,enabled) VALUES ('pi','default',{invalid})")).execute(&pool).await.is_err());
        }
        assert!(repository.get("pi", "default").await.unwrap().is_none());
        if existing {
            let state: String =
                sqlx::query_scalar("SELECT state FROM sessions WHERE session_id='existing'")
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            assert_eq!(state, "idle");
        }
        pool.close().await;
    }
}

#[tokio::test]
async fn writes_preserve_creation_time_and_update_modification_time() {
    let root = tempfile::tempdir().unwrap();
    let pool = connect_sqlite(&format!("sqlite://{}", root.path().join("db").display()))
        .await
        .unwrap();
    run_migrations(&pool).await.unwrap();
    let repository = SqliteAgentClientRepository::new(pool.clone());
    repository.save("pi", "default", false).await.unwrap();
    sqlx::query("UPDATE agent_clients SET created_at='2000-01-01T00:00:00Z', updated_at='2000-01-01T00:00:00Z'").execute(&pool).await.unwrap();
    repository.save("pi", "default", true).await.unwrap();
    let row = repository.get("pi", "default").await.unwrap().unwrap();
    assert_eq!(row.created_at, "2000-01-01T00:00:00Z");
    assert_ne!(row.updated_at, row.created_at);
    assert!(row.enabled);
    repository.delete("pi", "default").await.unwrap();
    repository.delete("pi", "default").await.unwrap();
    assert!(repository.get("pi", "default").await.unwrap().is_none());
    pool.close().await;
}

#[tokio::test]
async fn dropping_configuration_overrides_preserves_client_entries() {
    let root = tempfile::tempdir().unwrap();
    let pool = connect_sqlite(&format!("sqlite://{}", root.path().join("db").display()))
        .await
        .unwrap();
    sqlx::migrate::Migrator {
        migrations: std::borrow::Cow::Owned(
            sqlx::migrate!("./migrations")
                .iter()
                .filter(|migration| migration.version < 64)
                .cloned()
                .collect(),
        ),
        ..sqlx::migrate::Migrator::DEFAULT
    }
    .run(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO agent_clients(client_type,client_key,enabled,config_overrides)
        VALUES ('pi','default',0,'{\"tui_command\":\"database-pi\"}'), ('codex','default',1,'{}')",
    )
    .execute(&pool)
    .await
    .unwrap();
    run_migrations(&pool).await.unwrap();
    let rows = SqliteAgentClientRepository::new(pool.clone())
        .list()
        .await
        .unwrap();
    assert_eq!(
        rows.iter()
            .map(|row| (row.client_type.as_str(), row.enabled))
            .collect::<Vec<_>>(),
        [("codex", true), ("pi", false)]
    );
    assert!(
        rows.iter()
            .all(|row| row.version.is_none() && row.version_checked_at.is_none())
    );
    let columns: Vec<String> =
        sqlx::query_scalar("SELECT name FROM pragma_table_info('agent_clients')")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert!(!columns.iter().any(|column| column == "config_overrides"));
    pool.close().await;
}

#[tokio::test]
async fn version_checks_coalesce_and_only_the_current_attempt_can_record_a_version() {
    let pool = connect_sqlite("sqlite::memory:").await.unwrap();
    run_migrations(&pool).await.unwrap();
    let repo = SqliteAgentClientRepository::new(pool);
    let client = "pi";
    let key = "default";
    repo.save(client, key, true).await.unwrap();
    let old = "2000-01-01T00:00:00Z";
    let recent = "2999-01-01T00:00:00Z";
    assert!(
        repo.begin_version_check(client, key, old, false)
            .await
            .unwrap()
    );
    assert!(
        repo.begin_version_check(client, key, recent, false)
            .await
            .unwrap()
    );
    assert!(
        !repo
            .begin_version_check(client, key, recent, false)
            .await
            .unwrap()
    );
    repo.record_version(client, key, recent, "1.2.3")
        .await
        .unwrap();
    repo.record_version(client, key, old, "1.0.0")
        .await
        .unwrap();
    assert_eq!(
        repo.get(client, key)
            .await
            .unwrap()
            .unwrap()
            .version
            .as_deref(),
        Some("1.2.3")
    );
    repo.delete(client, key).await.unwrap();
    repo.record_version(client, key, recent, "2.0.0")
        .await
        .unwrap();
    assert!(repo.get(client, key).await.unwrap().is_none());
}

#[tokio::test]
async fn clearing_a_model_catalog_only_affects_the_selected_entry() {
    let pool = connect_sqlite("sqlite::memory:").await.unwrap();
    run_migrations(&pool).await.unwrap();
    let repository = SqliteAgentClientRepository::new(pool.clone());
    for key in ["kiro", "grok"] {
        repository.save("acp", key, true).await.unwrap();
        repository
            .save_model_catalog("acp", key, None, r#"{"fetched_at":1}"#, 1)
            .await
            .unwrap();
    }
    repository.clear_model_catalog("acp", "kiro").await.unwrap();
    assert_eq!(repository.model_catalog("acp", "kiro").await.unwrap(), None);
    assert_eq!(
        repository
            .model_catalog("acp", "grok")
            .await
            .unwrap()
            .as_deref(),
        Some(r#"{"fetched_at":1}"#)
    );
    repository
        .clear_model_catalog("acp", "missing")
        .await
        .unwrap();
    pool.close().await;
}
