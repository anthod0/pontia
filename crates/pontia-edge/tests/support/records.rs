use pontia_tunnel::DeviceIdentity;
use sqlx::SqlitePool;

pub async fn device(pool: &SqlitePool, identity: &DeviceIdentity) {
    sqlx::query("INSERT INTO devices (device_id, public_key) VALUES (?, ?)")
        .bind(identity.device_id().to_string())
        .bind(identity.public_key().as_slice())
        .execute(pool)
        .await
        .unwrap();
}
