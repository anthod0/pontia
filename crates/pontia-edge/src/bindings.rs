use std::path::Path;

use anyhow::{Result, ensure};
use ed25519_dalek::VerifyingKey;
use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
};
use uuid::Uuid;

#[derive(Clone)]
pub struct DeviceBindings {
    pool: SqlitePool,
}

impl DeviceBindings {
    pub async fn open(path: &Path) -> Result<Self> {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal);
        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(options)
            .await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        Ok(Self { pool })
    }

    pub async fn bind(
        &self,
        account_id: &str,
        device_id: Uuid,
        public_key: [u8; 32],
    ) -> Result<()> {
        let key = VerifyingKey::from_bytes(&public_key)?;
        ensure!(!key.is_weak(), "weak device public key");
        sqlx::query(
            "INSERT INTO device_bindings (account_id, device_id, public_key) VALUES (?, ?, ?)",
        )
        .bind(account_id)
        .bind(device_id.to_string())
        .bind(public_key.as_slice())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn public_key(&self, device_id: Uuid) -> Result<Option<[u8; 32]>> {
        let key: Option<Vec<u8>> =
            sqlx::query_scalar("SELECT public_key FROM device_bindings WHERE device_id = ?")
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
