use std::path::Path;

use anyhow::Result;
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

    pub async fn public_key(&self, device_id: Uuid) -> Result<Option<[u8; 32]>> {
        let key: Option<Vec<u8>> =
            sqlx::query_scalar("SELECT public_key FROM devices WHERE device_id = ?")
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
