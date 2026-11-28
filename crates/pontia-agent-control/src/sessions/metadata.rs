use pontia_core::Result;
use pontia_storage_sqlite::repositories::sessions::SqliteSessionRepository;
use serde_json::Value;
use sqlx::SqlitePool;

#[derive(Clone)]
pub struct SessionMetadataStore {
    pool: SqlitePool,
}

impl SessionMetadataStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn patch_keys(&self, session: &str, patches: &[(&str, Value)]) -> Result<()> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        SqliteSessionRepository::patch_metadata_keys_in_tx(&mut tx, session, patches).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn remove_keys(&self, session: &str, keys: &[&str]) -> Result<()> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        SqliteSessionRepository::remove_metadata_keys_in_tx(&mut tx, session, keys).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn update_key(
        &self,
        session: &str,
        key: &str,
        update: impl FnOnce(Option<Value>) -> Result<Value>,
    ) -> Result<()> {
        self.modify_key(session, key, |old| Ok((Some(update(old)?), ())))
            .await
    }

    /// Return None for the replacement value to leave the key and revision untouched.
    pub async fn modify_key<T>(
        &self,
        session: &str,
        key: &str,
        update: impl FnOnce(Option<Value>) -> Result<(Option<Value>, T)>,
    ) -> Result<T> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let result =
            SqliteSessionRepository::modify_metadata_key_in_tx(&mut tx, session, key, update)
                .await?;
        tx.commit().await?;
        Ok(result)
    }
}
