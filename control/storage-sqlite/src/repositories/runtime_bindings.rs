use pontia_core::{Error, Result};
use sqlx::{Sqlite, SqlitePool, Transaction};

#[derive(Debug, Clone)]
pub struct RuntimeBindingUpsertRecord {
    pub session_id: String,
    pub runtime_kind: String,
    pub runtime_instance_id: Option<String>,
    pub start_command: Option<String>,
    pub launch_cwd: Option<String>,
    pub last_seen_at: Option<String>,
    pub tmux_socket_path: Option<String>,
    pub tmux_pane_id: Option<String>,
    pub metadata: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RuntimeBindingTmuxPaneRow {
    pub socket_path: Option<String>,
    pub pane_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SqliteRuntimeBindingRepository {
    pool: SqlitePool,
}

impl SqliteRuntimeBindingRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn upsert_binding(&self, binding: RuntimeBindingUpsertRecord) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        Self::upsert_binding_in_tx(&mut tx, binding).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn upsert_binding_in_tx(
        tx: &mut Transaction<'_, Sqlite>,
        binding: RuntimeBindingUpsertRecord,
    ) -> Result<()> {
        sqlx::query(
            r#"INSERT INTO runtime_bindings (
                   session_id,
                   runtime_kind,
                   runtime_instance_id,
                   start_command,
                   launch_cwd,
                   last_seen_at,
                   tmux_socket_path,
                   tmux_pane_id,
                   metadata
               )
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
               ON CONFLICT(session_id) DO UPDATE SET
                   runtime_kind = excluded.runtime_kind,
                   runtime_instance_id = excluded.runtime_instance_id,
                   start_command = excluded.start_command,
                   launch_cwd = excluded.launch_cwd,
                   last_seen_at = excluded.last_seen_at,
                   tmux_socket_path = excluded.tmux_socket_path,
                   tmux_pane_id = excluded.tmux_pane_id,
                   metadata = excluded.metadata,
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')"#,
        )
        .bind(binding.session_id)
        .bind(binding.runtime_kind)
        .bind(binding.runtime_instance_id)
        .bind(binding.start_command)
        .bind(binding.launch_cwd)
        .bind(binding.last_seen_at)
        .bind(binding.tmux_socket_path)
        .bind(binding.tmux_pane_id)
        .bind(binding.metadata)
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    pub async fn upsert_binding_guarded(&self, binding: RuntimeBindingUpsertRecord) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        crate::repositories::turns::SqliteTurnRepository::serialize_session_turn_writes_in_tx(
            &mut tx,
            &binding.session_id,
        )
        .await?;
        Self::ensure_runtime_owner_may_write_in_tx(
            &mut tx,
            &binding.session_id,
            binding.runtime_instance_id.as_deref(),
        )
        .await?;
        Self::upsert_binding_in_tx(&mut tx, binding).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn ensure_runtime_owner_may_write_in_tx(
        tx: &mut Transaction<'_, Sqlite>,
        session_id: &str,
        incoming_runtime_instance_id: Option<&str>,
    ) -> Result<()> {
        let active_turn =
            crate::repositories::turns::SqliteTurnRepository::active_turn_in_tx(tx, session_id)
                .await?;
        if active_turn.is_none() {
            return Ok(());
        }
        let existing_runtime_instance_id = Self::runtime_instance_id_in_tx(tx, session_id).await?;
        let same_runtime_owner = incoming_runtime_instance_id
            .zip(existing_runtime_instance_id.as_deref())
            .is_some_and(|(incoming, existing)| incoming == existing);
        if !same_runtime_owner {
            return Err(Error::StateConflict(format!(
                "session {session_id} has an active Turn and is owned by another runtime"
            )));
        }
        Ok(())
    }

    pub async fn metadata(&self, session_id: &str) -> Result<Option<String>> {
        Ok(
            sqlx::query_scalar("SELECT metadata FROM runtime_bindings WHERE session_id = ?")
                .bind(session_id)
                .fetch_optional(&self.pool)
                .await?,
        )
    }

    pub async fn metadata_in_tx(
        tx: &mut Transaction<'_, Sqlite>,
        session_id: &str,
    ) -> Result<Option<String>> {
        Ok(
            sqlx::query_scalar("SELECT metadata FROM runtime_bindings WHERE session_id = ?")
                .bind(session_id)
                .fetch_optional(&mut **tx)
                .await?,
        )
    }

    pub async fn update_metadata(&self, session_id: &str, metadata: &str) -> Result<bool> {
        let result = sqlx::query(
            "UPDATE runtime_bindings SET metadata = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE session_id = ?",
        )
        .bind(metadata)
        .bind(session_id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn runtime_instance_id(&self, session_id: &str) -> Result<Option<String>> {
        Ok(sqlx::query_scalar::<_, Option<String>>(
            "SELECT runtime_instance_id FROM runtime_bindings WHERE session_id = ?",
        )
        .bind(session_id)
        .fetch_optional(&self.pool)
        .await?
        .flatten())
    }

    pub async fn runtime_instance_id_in_tx(
        tx: &mut Transaction<'_, Sqlite>,
        session_id: &str,
    ) -> Result<Option<String>> {
        Ok(sqlx::query_scalar::<_, Option<String>>(
            "SELECT runtime_instance_id FROM runtime_bindings WHERE session_id = ?",
        )
        .bind(session_id)
        .fetch_optional(&mut **tx)
        .await?
        .flatten())
    }

    pub async fn tmux_pane_binding(
        &self,
        session_id: &str,
    ) -> Result<Option<RuntimeBindingTmuxPaneRow>> {
        Ok(sqlx::query_as::<_, RuntimeBindingTmuxPaneRow>(
            "SELECT tmux_socket_path AS socket_path, tmux_pane_id AS pane_id FROM runtime_bindings WHERE session_id = ?",
        )
        .bind(session_id)
        .fetch_optional(&self.pool)
        .await?)
    }

    pub async fn start_command(&self, session_id: &str) -> Result<Option<String>> {
        Ok(sqlx::query_scalar::<_, Option<String>>(
            "SELECT start_command FROM runtime_bindings WHERE session_id = ?",
        )
        .bind(session_id)
        .fetch_optional(&self.pool)
        .await?
        .flatten())
    }
}
