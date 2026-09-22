#[path = "support/records.rs"]
mod records;

use pontia_edge::DeviceRegistry;
use pontia_tunnel::DeviceIdentity;

#[tokio::test]
async fn seeded_device_access_survives_reopening_the_database() {
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
    records::access_key(&pool, "first", "first-secret", Some(first.device_id())).await;
    records::access_key(&pool, "second", "second-secret", Some(second.device_id())).await;
    drop(devices);
    let devices = DeviceRegistry::open(&path).await.unwrap();

    for (secret, identity) in [("first-secret", &first), ("second-secret", &second)] {
        assert_eq!(
            devices
                .authorized_public_key(secret, identity.device_id())
                .await
                .unwrap(),
            Some(identity.public_key())
        );
    }
}
