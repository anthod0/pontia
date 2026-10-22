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
        for peer in ["kimi", "company"] {
            repository.save("acp", peer, None, None).await.unwrap();
        }
        assert_eq!(repository.list().await.unwrap().len(), 2);
        let kimi = repository.get("acp", "kimi").await.unwrap().unwrap();
        assert!(kimi.enabled);
        assert_eq!(kimi.config_overrides, "{}");
        assert!(!kimi.created_at.is_empty());
        assert_eq!(kimi.created_at, kimi.updated_at);
        assert!(
            sqlx::query("INSERT INTO agent_clients(client_type,client_key) VALUES ('acp','kimi')")
                .execute(&pool)
                .await
                .is_err()
        );
        for invalid in ["not json", "[]", "null", "1", "\"text\""] {
            assert!(
                repository
                    .save("pi", "default", None, Some(invalid))
                    .await
                    .is_err()
            );
        }
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
async fn writes_preserve_unsupplied_settings_and_creation_time_and_update_modification_time() {
    let root = tempfile::tempdir().unwrap();
    let pool = connect_sqlite(&format!("sqlite://{}", root.path().join("db").display()))
        .await
        .unwrap();
    run_migrations(&pool).await.unwrap();
    let repository = SqliteAgentClientRepository::new(pool.clone());
    repository
        .save(
            "pi",
            "default",
            Some(false),
            Some(r#"{"tui_command":"custom-pi"}"#),
        )
        .await
        .unwrap();
    sqlx::query("UPDATE agent_clients SET created_at='2000-01-01T00:00:00Z', updated_at='2000-01-01T00:00:00Z'").execute(&pool).await.unwrap();
    repository
        .save("pi", "default", Some(true), None)
        .await
        .unwrap();
    let row = repository.get("pi", "default").await.unwrap().unwrap();
    assert_eq!(row.created_at, "2000-01-01T00:00:00Z");
    assert_ne!(row.updated_at, row.created_at);
    assert!(row.enabled);
    assert_eq!(row.config_overrides, r#"{"tui_command":"custom-pi"}"#);
    repository
        .save("pi", "default", None, Some("{}"))
        .await
        .unwrap();
    let row = repository.get("pi", "default").await.unwrap().unwrap();
    assert!(row.enabled);
    assert_eq!(row.config_overrides, "{}");
    repository.delete("pi", "default").await.unwrap();
    repository.delete("pi", "default").await.unwrap();
    assert!(repository.get("pi", "default").await.unwrap().is_none());
    pool.close().await;
}
