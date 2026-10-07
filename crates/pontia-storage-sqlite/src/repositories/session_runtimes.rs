use pontia_core::{Error, Result};
use serde::{Deserialize, Serialize};
use sqlx::{Sqlite, SqlitePool, Transaction};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::FromRow)]
pub struct SessionRuntimeRecord {
    pub runtime_id: String,
    pub session_id: String,
    pub role: String,
    pub state: String,
    pub start_command: Option<String>,
    pub tmux_socket_path: Option<String>,
    pub tmux_pane_id: Option<String>,
    pub process_fingerprint: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RuntimeBindingTmuxPaneRow {
    pub runtime_id: Option<String>,
    pub socket_path: Option<String>,
    pub pane_id: Option<String>,
    pub process_fingerprint: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ActiveTmuxProcessBindingRow {
    pub session_id: String,
    pub client_type: String,
    pub runtime_id: String,
    pub socket_path: String,
    pub pane_id: String,
    pub process_fingerprint: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SqliteSessionRuntimeRepository {
    pool: SqlitePool,
}

impl SqliteSessionRuntimeRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn list(&self, session_id: &str) -> Result<Vec<SessionRuntimeRecord>> {
        Ok(sqlx::query_as(
            "SELECT * FROM session_runtimes WHERE session_id = ? ORDER BY created_at, runtime_id",
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn get(&self, runtime_id: &str) -> Result<Option<SessionRuntimeRecord>> {
        Ok(
            sqlx::query_as("SELECT * FROM session_runtimes WHERE runtime_id = ?")
                .bind(runtime_id)
                .fetch_optional(&self.pool)
                .await?,
        )
    }

    /// Writes exactly one stable runtime; ownership and creation time cannot change.
    pub async fn upsert_binding_in_tx(
        tx: &mut Transaction<'_, Sqlite>,
        record: SessionRuntimeRecord,
    ) -> Result<()> {
        let result = sqlx::query(r#"INSERT INTO session_runtimes
            (runtime_id, session_id, role, state, start_command, tmux_socket_path, tmux_pane_id, process_fingerprint, created_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(runtime_id) DO UPDATE SET
                state = excluded.state,
                start_command = COALESCE(excluded.start_command, session_runtimes.start_command),
                tmux_socket_path = excluded.tmux_socket_path,
                tmux_pane_id = excluded.tmux_pane_id,
                process_fingerprint = excluded.process_fingerprint
            WHERE session_runtimes.session_id = excluded.session_id AND session_runtimes.role = excluded.role"#)
            .bind(record.runtime_id).bind(record.session_id).bind(record.role).bind(record.state)
            .bind(record.start_command).bind(record.tmux_socket_path).bind(record.tmux_pane_id)
            .bind(record.process_fingerprint).bind(record.created_at).execute(&mut **tx).await?;
        if result.rows_affected() != 1 {
            return Err(Error::StateConflict(
                "runtime ownership cannot change".into(),
            ));
        }
        Ok(())
    }

    pub async fn upsert_binding(&self, record: SessionRuntimeRecord) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        Self::upsert_binding_in_tx(&mut tx, record).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn upsert_binding_guarded(&self, record: SessionRuntimeRecord) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        crate::repositories::turns::SqliteTurnRepository::serialize_session_turn_writes_in_tx(
            &mut tx,
            &record.session_id,
        )
        .await?;
        Self::ensure_runtime_owner_may_write_in_tx(
            &mut tx,
            &record.session_id,
            Some(&record.runtime_id),
        )
        .await?;
        Self::upsert_binding_in_tx(&mut tx, record).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn ensure_runtime_owner_may_write_in_tx(
        tx: &mut Transaction<'_, Sqlite>,
        session_id: &str,
        incoming: Option<&str>,
    ) -> Result<()> {
        let existing = Self::runtime_id_in_tx(tx, session_id).await?;
        if existing.is_some() && existing.as_deref() != incoming {
            return Err(Error::StateConflict(
                "Session already has a different control runtime".into(),
            ));
        }
        Ok(())
    }

    /// Pi has one TUI. Ambiguity is an error, never an arbitrary row choice.
    pub async fn runtime_id(&self, session_id: &str) -> Result<Option<String>> {
        let ids: Vec<String> = sqlx::query_scalar(
            "SELECT runtime_id FROM session_runtimes WHERE session_id = ? AND role = 'tui'",
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?;
        single_runtime(ids)
    }

    pub async fn runtime_id_in_tx(
        tx: &mut Transaction<'_, Sqlite>,
        session_id: &str,
    ) -> Result<Option<String>> {
        let ids: Vec<String> = sqlx::query_scalar(
            "SELECT runtime_id FROM session_runtimes WHERE session_id = ? AND role = 'tui'",
        )
        .bind(session_id)
        .fetch_all(&mut **tx)
        .await?;
        single_runtime(ids)
    }

    pub async fn tmux_pane_binding(
        &self,
        session_id: &str,
    ) -> Result<Option<RuntimeBindingTmuxPaneRow>> {
        let Some(id) = self.runtime_id(session_id).await? else {
            return Ok(None);
        };
        Ok(sqlx::query_as("SELECT runtime_id, tmux_socket_path AS socket_path, tmux_pane_id AS pane_id, process_fingerprint FROM session_runtimes WHERE runtime_id = ?")
            .bind(id).fetch_optional(&self.pool).await?)
    }

    pub async fn active_tmux_process_bindings(&self) -> Result<Vec<ActiveTmuxProcessBindingRow>> {
        Ok(sqlx::query_as(
            r#"SELECT s.session_id, s.client_type, r.runtime_id, r.tmux_socket_path AS socket_path,
            r.tmux_pane_id AS pane_id, r.process_fingerprint FROM sessions s
            JOIN session_runtimes r ON r.session_id = s.session_id
            WHERE s.client_type = 'pi' AND r.role = 'tui' AND r.state = 'running'
              AND r.tmux_socket_path IS NOT NULL AND r.tmux_pane_id IS NOT NULL"#,
        )
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn start_command(&self, session_id: &str) -> Result<Option<String>> {
        let Some(id) = self.runtime_id(session_id).await? else {
            return Ok(None);
        };
        Ok(self.get(&id).await?.and_then(|row| row.start_command))
    }
}

fn single_runtime(ids: Vec<String>) -> Result<Option<String>> {
    match ids.len() {
        0 => Ok(None),
        1 => Ok(ids.into_iter().next()),
        _ => Err(Error::StateConflict(
            "Session has multiple TUI runtimes; control target is ambiguous".into(),
        )),
    }
}
