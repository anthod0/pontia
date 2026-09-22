use pontia_tunnel::DeviceIdentity;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use uuid::Uuid;

pub async fn device(pool: &SqlitePool, identity: &DeviceIdentity) {
    sqlx::query("INSERT INTO devices (device_id, public_key) VALUES (?, ?)")
        .bind(identity.device_id().to_string())
        .bind(identity.public_key().as_slice())
        .execute(pool)
        .await
        .unwrap();
}

pub async fn access_key(pool: &SqlitePool, key_id: &str, secret: &str, device_id: Option<Uuid>) {
    sqlx::query("INSERT INTO access_keys (key_id, secret_hash, device_id) VALUES (?, ?, ?)")
        .bind(key_id)
        .bind(Sha256::digest(secret.as_bytes()).as_slice())
        .bind(device_id.map(|id| id.to_string()))
        .execute(pool)
        .await
        .unwrap();
}
