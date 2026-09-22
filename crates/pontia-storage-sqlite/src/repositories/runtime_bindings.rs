use pontia_core::{Error, Result};
use sqlx::{Sqlite, SqlitePool, Transaction};

#[derive(Debug, Clone)]
pub struct RuntimeBindingUpsertRecord {
    pub session_id: String,
    pub runtime_kind: String,
    pub runtime_instance_id: Option<String>,
    pub binding_state: String,
    pub runtime_handle: Option<String>,
    pub start_command: Option<String>,
    pub launch_cwd: Option<String>,
    pub internal_event_url: Option<String>,
    pub started_at: Option<String>,
    pub last_seen_at: Option<String>,
    pub restart_count: i64,
    pub tmux_socket_path: Option<String>,
    pub tmux_pane_id: Option<String>,
    pub process_fingerprint: Option<String>,
    pub capabilities: String,
    pub diagnostics: String,
    pub adapter_details: String,
}

#[derive(Debug, Clone)]
pub struct RuntimeBindingConfirmationRecord {
    pub session_id: String,
    pub runtime_kind: String,
    pub runtime_instance_id: String,
    pub start_command: Option<String>,
    pub launch_cwd: String,
    pub internal_event_url: String,
    pub last_seen_at: String,
    pub tmux_socket_path: Option<String>,
    pub tmux_pane_id: Option<String>,
    pub process_fingerprint: Option<String>,
    pub capabilities: String,
    pub diagnostics: String,
    pub adapter_details: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RuntimeBindingTmuxPaneRow {
    pub runtime_instance_id: Option<String>,
    pub socket_path: Option<String>,
    pub pane_id: Option<String>,
    pub process_fingerprint: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RuntimeTurnContextRow {
    pub runtime_instance_id: Option<String>,
    pub internal_event_url: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ActiveTmuxProcessBindingRow {
    pub session_id: String,
    pub client_type: String,
    pub runtime_instance_id: String,
    pub socket_path: String,
    pub pane_id: String,
    pub process_fingerprint: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PendingTurnContextRecord {
    pub session_id: String,
    pub runtime_instance_id: String,
    pub client_type: String,
    pub payload: String,
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
                   session_id, runtime_kind, runtime_instance_id, binding_state,
                   runtime_handle, start_command, launch_cwd, internal_event_url,
                   started_at, last_seen_at, restart_count,
                   tmux_socket_path, tmux_pane_id, process_fingerprint,
                   capabilities, diagnostics, adapter_details
               )
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
               ON CONFLICT(session_id) DO UPDATE SET
                   runtime_kind = excluded.runtime_kind,
                   runtime_instance_id = excluded.runtime_instance_id,
                   binding_state = CASE
                       WHEN runtime_bindings.runtime_instance_id = excluded.runtime_instance_id
                        AND runtime_bindings.binding_state = 'confirmed'
                        AND excluded.binding_state = 'provisioned'
                       THEN runtime_bindings.binding_state
                       ELSE excluded.binding_state
                   END,
                   runtime_handle = excluded.runtime_handle,
                   start_command = excluded.start_command,
                   launch_cwd = excluded.launch_cwd,
                   internal_event_url = CASE
                       WHEN runtime_bindings.runtime_instance_id = excluded.runtime_instance_id
                        AND runtime_bindings.binding_state = 'confirmed'
                        AND excluded.binding_state = 'provisioned'
                       THEN runtime_bindings.internal_event_url
                       ELSE excluded.internal_event_url
                   END,
                   started_at = excluded.started_at,
                   last_seen_at = CASE
                       WHEN runtime_bindings.runtime_instance_id = excluded.runtime_instance_id
                        AND runtime_bindings.binding_state = 'confirmed'
                        AND excluded.binding_state = 'provisioned'
                       THEN runtime_bindings.last_seen_at
                       ELSE excluded.last_seen_at
                   END,
                   restart_count = excluded.restart_count,
                   tmux_socket_path = CASE
                       WHEN runtime_bindings.runtime_instance_id = excluded.runtime_instance_id
                        AND runtime_bindings.binding_state = 'confirmed'
                        AND excluded.binding_state = 'provisioned'
                       THEN runtime_bindings.tmux_socket_path
                       ELSE excluded.tmux_socket_path
                   END,
                   tmux_pane_id = CASE
                       WHEN runtime_bindings.runtime_instance_id = excluded.runtime_instance_id
                        AND runtime_bindings.binding_state = 'confirmed'
                        AND excluded.binding_state = 'provisioned'
                       THEN runtime_bindings.tmux_pane_id
                       ELSE excluded.tmux_pane_id
                   END,
                   process_fingerprint = CASE
                       WHEN runtime_bindings.runtime_instance_id = excluded.runtime_instance_id
                        AND runtime_bindings.binding_state = 'confirmed'
                        AND excluded.binding_state = 'provisioned'
                       THEN runtime_bindings.process_fingerprint
                       ELSE excluded.process_fingerprint
                   END,
                   capabilities = CASE
                       WHEN runtime_bindings.runtime_instance_id = excluded.runtime_instance_id
                        AND runtime_bindings.binding_state = 'confirmed'
                        AND excluded.binding_state = 'provisioned'
                       THEN runtime_bindings.capabilities
                       ELSE excluded.capabilities
                   END,
                   diagnostics = CASE
                       WHEN runtime_bindings.runtime_instance_id = excluded.runtime_instance_id
                        AND runtime_bindings.binding_state = 'confirmed'
                        AND excluded.binding_state = 'provisioned'
                       THEN runtime_bindings.diagnostics
                       ELSE excluded.diagnostics
                   END,
                   adapter_details = CASE
                       WHEN runtime_bindings.runtime_instance_id = excluded.runtime_instance_id
                        AND runtime_bindings.binding_state = 'confirmed'
                        AND excluded.binding_state = 'provisioned'
                       THEN runtime_bindings.adapter_details
                       ELSE excluded.adapter_details
                   END,
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')"#,
        )
        .bind(binding.session_id)
        .bind(binding.runtime_kind)
        .bind(binding.runtime_instance_id)
        .bind(binding.binding_state)
        .bind(binding.runtime_handle)
        .bind(binding.start_command)
        .bind(binding.launch_cwd)
        .bind(binding.internal_event_url)
        .bind(binding.started_at)
        .bind(binding.last_seen_at)
        .bind(binding.restart_count)
        .bind(binding.tmux_socket_path)
        .bind(binding.tmux_pane_id)
        .bind(binding.process_fingerprint)
        .bind(binding.capabilities)
        .bind(binding.diagnostics)
        .bind(binding.adapter_details)
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    pub async fn confirm_binding_in_tx(
        tx: &mut Transaction<'_, Sqlite>,
        binding: RuntimeBindingConfirmationRecord,
    ) -> Result<()> {
        sqlx::query(
            r#"INSERT INTO runtime_bindings (
                   session_id, runtime_kind, runtime_instance_id, binding_state,
                   start_command, launch_cwd, internal_event_url, last_seen_at,
                   tmux_socket_path, tmux_pane_id, process_fingerprint,
                   capabilities, diagnostics, adapter_details
               )
               VALUES (?, ?, ?, 'confirmed', ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
               ON CONFLICT(session_id) DO UPDATE SET
                   runtime_kind = excluded.runtime_kind,
                   runtime_instance_id = excluded.runtime_instance_id,
                   binding_state = 'confirmed',
                   start_command = COALESCE(excluded.start_command, runtime_bindings.start_command),
                   launch_cwd = excluded.launch_cwd,
                   internal_event_url = excluded.internal_event_url,
                   last_seen_at = excluded.last_seen_at,
                   tmux_socket_path = excluded.tmux_socket_path,
                   tmux_pane_id = excluded.tmux_pane_id,
                   process_fingerprint = excluded.process_fingerprint,
                   capabilities = excluded.capabilities,
                   diagnostics = json_patch(runtime_bindings.diagnostics, excluded.diagnostics),
                   adapter_details = json_patch(runtime_bindings.adapter_details, excluded.adapter_details),
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')"#,
        )
        .bind(binding.session_id)
        .bind(binding.runtime_kind)
        .bind(binding.runtime_instance_id)
        .bind(binding.start_command)
        .bind(binding.launch_cwd)
        .bind(binding.internal_event_url)
        .bind(binding.last_seen_at)
        .bind(binding.tmux_socket_path)
        .bind(binding.tmux_pane_id)
        .bind(binding.process_fingerprint)
        .bind(binding.capabilities)
        .bind(binding.diagnostics)
        .bind(binding.adapter_details)
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

    pub async fn runtime_context(&self, session_id: &str) -> Result<Option<RuntimeTurnContextRow>> {
        Ok(sqlx::query_as(
            "SELECT runtime_instance_id, internal_event_url FROM runtime_bindings WHERE session_id = ?",
        )
        .bind(session_id)
        .fetch_optional(&self.pool)
        .await?)
    }

    pub async fn launch_cwd_in_tx(
        tx: &mut Transaction<'_, Sqlite>,
        session_id: &str,
    ) -> Result<Option<String>> {
        Ok(
            sqlx::query_scalar("SELECT launch_cwd FROM runtime_bindings WHERE session_id = ?")
                .bind(session_id)
                .fetch_optional(&mut **tx)
                .await?
                .flatten(),
        )
    }

    pub async fn runtime_handle(&self, session_id: &str) -> Result<Option<String>> {
        Ok(
            sqlx::query_scalar("SELECT runtime_handle FROM runtime_bindings WHERE session_id = ?")
                .bind(session_id)
                .fetch_optional(&self.pool)
                .await?
                .flatten(),
        )
    }

    pub async fn restart_count(&self, session_id: &str) -> Result<Option<i64>> {
        Ok(
            sqlx::query_scalar("SELECT restart_count FROM runtime_bindings WHERE session_id = ?")
                .bind(session_id)
                .fetch_optional(&self.pool)
                .await?,
        )
    }

    pub async fn tmux_pane_binding(
        &self,
        session_id: &str,
    ) -> Result<Option<RuntimeBindingTmuxPaneRow>> {
        Ok(sqlx::query_as::<_, RuntimeBindingTmuxPaneRow>(
            "SELECT runtime_instance_id, tmux_socket_path AS socket_path, tmux_pane_id AS pane_id, process_fingerprint FROM runtime_bindings WHERE session_id = ?",
        )
        .bind(session_id)
        .fetch_optional(&self.pool)
        .await?)
    }

    pub async fn active_tmux_process_bindings(&self) -> Result<Vec<ActiveTmuxProcessBindingRow>> {
        Ok(sqlx::query_as::<_, ActiveTmuxProcessBindingRow>(
            r#"SELECT s.session_id,
                      s.client_type,
                      r.runtime_instance_id,
                      r.tmux_socket_path AS socket_path,
                      r.tmux_pane_id AS pane_id,
                      r.process_fingerprint
               FROM sessions s
               JOIN runtime_bindings r ON r.session_id = s.session_id
               WHERE s.state IN ('idle', 'busy', 'interrupted')
                 AND r.runtime_instance_id IS NOT NULL
                 AND r.tmux_socket_path IS NOT NULL
                 AND r.tmux_pane_id IS NOT NULL"#,
        )
        .fetch_all(&self.pool)
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

    pub async fn store_pending_turn_context(
        &self,
        context: PendingTurnContextRecord,
    ) -> Result<()> {
        let result = sqlx::query(
            r#"INSERT INTO pending_turn_contexts
                   (session_id, runtime_instance_id, client_type, payload)
               SELECT ?, ?, ?, ?
               WHERE EXISTS (
                   SELECT 1 FROM runtime_bindings
                   WHERE session_id = ? AND runtime_instance_id = ?
               )
               ON CONFLICT(session_id) DO UPDATE SET
                   runtime_instance_id = excluded.runtime_instance_id,
                   client_type = excluded.client_type,
                   payload = excluded.payload,
                   created_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')"#,
        )
        .bind(&context.session_id)
        .bind(&context.runtime_instance_id)
        .bind(context.client_type)
        .bind(context.payload)
        .bind(&context.session_id)
        .bind(&context.runtime_instance_id)
        .execute(&self.pool)
        .await?;
        if result.rows_affected() == 0 {
            return Err(Error::StateConflict(
                "runtime_instance_id does not match active runtime binding".to_string(),
            ));
        }
        Ok(())
    }

    pub async fn claim_pending_turn_context(
        &self,
        session_id: &str,
        runtime_instance_id: &str,
        client_type: &str,
    ) -> Result<Option<String>> {
        Ok(sqlx::query_scalar(
            r#"DELETE FROM pending_turn_contexts
               WHERE session_id = ? AND runtime_instance_id = ? AND client_type = ?
                 AND EXISTS (
                     SELECT 1 FROM runtime_bindings
                     WHERE runtime_bindings.session_id = pending_turn_contexts.session_id
                       AND runtime_bindings.runtime_instance_id = pending_turn_contexts.runtime_instance_id
                 )
               RETURNING payload"#,
        )
        .bind(session_id)
        .bind(runtime_instance_id)
        .bind(client_type)
        .fetch_optional(&self.pool)
        .await?)
    }
}
