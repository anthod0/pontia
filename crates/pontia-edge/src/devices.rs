use std::path::Path;

use anyhow::Result;
use sha2::{Digest, Sha256};
use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
};
use uuid::Uuid;

#[derive(Clone)]
pub struct DeviceRegistry {
    pool: SqlitePool,
}

impl DeviceRegistry {
    pub async fn open(path: &Path) -> Result<Self> {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .foreign_keys(true)
            .journal_mode(SqliteJournalMode::Wal);
        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(options)
            .await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        Ok(Self { pool })
    }

    pub async fn authorized_public_key(
        &self,
        access_key: &str,
        device_id: Uuid,
    ) -> Result<Option<[u8; 32]>> {
        if access_key.trim().is_empty()
            || access_key.len() > pontia_tunnel::protocol::MAX_ACCESS_KEY_BYTES
        {
            return Ok(None);
        }
        let key: Option<Vec<u8>> = sqlx::query_scalar(
            "SELECT devices.public_key FROM access_keys
             JOIN devices ON devices.device_id = access_keys.device_id
             WHERE access_keys.secret_hash = ? AND devices.device_id = ?",
        )
        .bind(Sha256::digest(access_key.as_bytes()).as_slice())
        .bind(device_id.to_string())
        .fetch_optional(&self.pool)
        .await?;
        key.map(|key| {
            key.try_into()
                .map_err(|_| anyhow::anyhow!("invalid stored device key"))
        })
        .transpose()
    }
}
