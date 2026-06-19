use pontia_storage_sqlite::{
    connect_sqlite, repositories::idempotency::SqliteIdempotencyRepository, run_migrations,
};

async fn test_pool() -> sqlx::SqlitePool {
    let db = format!(
        "sqlite://{}",
        tempfile::NamedTempFile::new().unwrap().path().display()
    );
    let pool = connect_sqlite(&db).await.expect("connect");
    run_migrations(&pool).await.expect("migrate");
    pool
}

#[tokio::test]
async fn stores_and_replays_json_response_for_operation_key() {
    let pool = test_pool().await;
    let repository = SqliteIdempotencyRepository::new(pool);
    let response = serde_json::json!({"ok": true, "value": 42});

    assert_eq!(
        repository
            .get_response("create_session", "retry-key")
            .await
            .unwrap(),
        None
    );

    repository
        .store_response("create_session", "retry-key", &response)
        .await
        .unwrap();

    assert_eq!(
        repository
            .get_response("create_session", "retry-key")
            .await
            .unwrap(),
        Some(response)
    );
}

#[tokio::test]
async fn keeps_first_response_when_same_operation_key_is_stored_again() {
    let pool = test_pool().await;
    let repository = SqliteIdempotencyRepository::new(pool);
    let first = serde_json::json!({"attempt": 1});
    let second = serde_json::json!({"attempt": 2});

    repository
        .store_response("interrupt_turn", "same-key", &first)
        .await
        .unwrap();
    repository
        .store_response("interrupt_turn", "same-key", &second)
        .await
        .unwrap();

    assert_eq!(
        repository
            .get_response("interrupt_turn", "same-key")
            .await
            .unwrap(),
        Some(first)
    );
}
