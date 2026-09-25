#[path = "support/records.rs"]
mod records;

use pontia_edge::DeviceRegistry;
use pontia_tunnel::DeviceIdentity;

#[tokio::test]
async fn registered_device_keys_survive_reopening_the_database() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("edge.db");
    let devices = DeviceRegistry::open(&path).await.unwrap();
    let pool =
        sqlx::SqlitePool::connect_with(sqlx::sqlite::SqliteConnectOptions::new().filename(&path))
            .await
            .unwrap();
    let first = DeviceIdentity::generate().unwrap();
    let second = DeviceIdentity::generate().unwrap();
    records::device(&pool, &first).await;
    records::device(&pool, &second).await;
    drop(devices);
    let devices = DeviceRegistry::open(&path).await.unwrap();

    for identity in [&first, &second] {
        assert_eq!(
            devices.public_key(identity.device_id()).await.unwrap(),
            Some(identity.public_key())
        );
    }
    assert_eq!(
        devices
            .public_key(DeviceIdentity::generate().unwrap().device_id())
            .await
            .unwrap(),
        None
    );
}

#[tokio::test]
async fn migration_removes_legacy_keys_without_losing_registered_devices() {
    let root = tempfile::tempdir().unwrap();
    let migrations = root.path().join("migrations");
    std::fs::create_dir(&migrations).unwrap();
    std::fs::write(
        migrations.join("0001_devices.sql"),
        include_str!("../migrations/0001_devices.sql"),
    )
    .unwrap();
    let path = root.path().join("edge.db");
    let pool = sqlx::SqlitePool::connect_with(
        sqlx::sqlite::SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true),
    )
    .await
    .unwrap();
    sqlx::migrate::Migrator::new(migrations.as_path())
        .await
        .unwrap()
        .run(&pool)
        .await
        .unwrap();
    let identity = DeviceIdentity::generate().unwrap();
    records::device(&pool, &identity).await;
    sqlx::query("INSERT INTO access_keys (key_id, secret_hash, device_id) VALUES (?, ?, ?)")
        .bind("old-key")
        .bind(vec![1_u8; 32])
        .bind(identity.device_id().to_string())
        .execute(&pool)
        .await
        .unwrap();
    drop(pool);

    let registry = DeviceRegistry::open(&path).await.unwrap();
    assert_eq!(
        registry.public_key(identity.device_id()).await.unwrap(),
        Some(identity.public_key())
    );
    let pool =
        sqlx::SqlitePool::connect_with(sqlx::sqlite::SqliteConnectOptions::new().filename(&path))
            .await
            .unwrap();
    let old_table: Option<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'access_keys'",
    )
    .fetch_optional(&pool)
    .await
    .unwrap();
    assert!(old_table.is_none());
}
