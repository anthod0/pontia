use pontia_storage_sqlite::{
    connect_sqlite,
    repositories::agent_profiles::{AgentProfileWriteRecord, SqliteAgentProfileRepository},
    run_migrations,
};

fn record(name: &str) -> AgentProfileWriteRecord {
    AgentProfileWriteRecord {
        profile_id: "reviewer".into(),
        name: name.into(),
        description: None,
        client_type: None,
        client_key: None,
        model: None,
        system_prompt: Some("Review carefully".into()),
    }
}

#[tokio::test]
async fn creates_updates_and_deletes_profiles() {
    let pool = connect_sqlite("sqlite::memory:").await.unwrap();
    run_migrations(&pool).await.unwrap();
    let repository = SqliteAgentProfileRepository::new(pool);
    repository.create(record("Original")).await.unwrap();
    assert!(repository.create(record("Duplicate")).await.is_err());
    let original = repository.get("reviewer").await.unwrap().unwrap();
    assert_eq!(original.name, "Original");
    assert_eq!(
        repository.update(record("Edited")).await.unwrap(),
        Some(false)
    );
    let profiles = repository.list().await.unwrap();
    assert_eq!(profiles.len(), 1);
    assert_eq!(profiles[0].name, "Edited");
    assert_eq!(profiles[0].created_at, original.created_at);
    assert!(repository.delete("reviewer").await.unwrap());
    assert!(!repository.delete("reviewer").await.unwrap());
    assert_eq!(repository.update(record("Missing")).await.unwrap(), None);
    assert!(repository.get("reviewer").await.unwrap().is_none());
}

#[tokio::test]
async fn removing_builtin_profiles_preserves_user_profiles() {
    let pool = connect_sqlite("sqlite::memory:").await.unwrap();
    let previous = sqlx::migrate::Migrator {
        migrations: std::borrow::Cow::Owned(
            sqlx::migrate!("./migrations")
                .iter()
                .filter(|migration| migration.version < 51)
                .cloned()
                .collect(),
        ),
        ..sqlx::migrate::Migrator::DEFAULT
    };
    previous.run(&pool).await.unwrap();
    sqlx::query("INSERT INTO execution_profiles (profile_id, version, name, system_prompt_template) VALUES ('custom', '1', 'Custom', 'custom instructions')")
        .execute(&pool).await.unwrap();
    let custom_updated_at: String = sqlx::query_scalar(
        "SELECT updated_at FROM execution_profiles WHERE profile_id = 'custom' AND version = '1'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    run_migrations(&pool).await.unwrap();
    let repository = SqliteAgentProfileRepository::new(pool.clone());
    assert!(repository.get("default").await.unwrap().is_none());
    assert!(repository.get("default").await.unwrap().is_none());
    assert!(repository.get("default").await.unwrap().is_none());
    let custom_after = repository.get("custom").await.unwrap().unwrap();
    assert_eq!(
        custom_after.system_prompt.as_deref(),
        Some("custom instructions")
    );
    assert_eq!(custom_after.updated_at, custom_updated_at);
    assert_eq!(custom_after.client_type, None);
    assert_eq!(custom_after.client_key, None);
    assert_eq!(custom_after.model, None);
}
