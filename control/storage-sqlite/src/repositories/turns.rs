use sqlx::{Sqlite, SqlitePool, Transaction};

use crate::models::turns::{TurnEventEnrichmentRow, TurnProjectionRow, TurnRow};

use pontia_core::Result;

#[derive(Debug, Clone)]
pub struct TurnProjectionUpsertRecord {
    pub turn_id: String,
    pub session_id: String,
    pub state: String,
    pub state_version: i64,
    pub metadata: String,
}

#[derive(Debug, Clone)]
pub struct SqliteTurnRepository {
    pool: SqlitePool,
}

impl SqliteTurnRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn load_projection_rows(&self, session_id: &str) -> Result<Vec<TurnProjectionRow>> {
        Ok(sqlx::query_as::<_, TurnProjectionRow>(
            "SELECT turn_id, session_id, state, state_version, metadata FROM turns WHERE session_id = ?",
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn get_projection(&self, turn_id: &str) -> Result<Option<TurnProjectionRow>> {
        Ok(sqlx::query_as::<_, TurnProjectionRow>(
            "SELECT turn_id, session_id, state, state_version, metadata FROM turns WHERE turn_id = ?",
        )
        .bind(turn_id)
        .fetch_optional(&self.pool)
        .await?)
    }

    pub async fn upsert_projection_in_tx(
        tx: &mut Transaction<'_, Sqlite>,
        turn: TurnProjectionUpsertRecord,
    ) -> Result<()> {
        sqlx::query(
            r#"INSERT INTO turns
               (turn_id, session_id, state, state_version, metadata)
               VALUES (?, ?, ?, ?, ?)
               ON CONFLICT(turn_id) DO UPDATE SET
                   session_id = excluded.session_id,
                   state = excluded.state,
                   state_version = excluded.state_version,
                   metadata = excluded.metadata,
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')"#,
        )
        .bind(turn.turn_id)
        .bind(turn.session_id)
        .bind(turn.state)
        .bind(turn.state_version)
        .bind(turn.metadata)
        .execute(&mut **tx)
        .await?;

        Ok(())
    }

    pub async fn list_turns(&self, session_id: &str) -> Result<Vec<TurnRow>> {
        Ok(sqlx::query_as::<_, TurnRow>(
            r#"SELECT turn_id, session_id, state, input_summary, output_summary,
                      failure_message, metadata, created_at, updated_at
               FROM turns WHERE session_id = ? ORDER BY created_at, turn_id"#,
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn get_turn(&self, session_id: &str, turn_id: &str) -> Result<Option<TurnRow>> {
        Ok(sqlx::query_as::<_, TurnRow>(
            r#"SELECT turn_id, session_id, state, input_summary, output_summary,
                      failure_message, metadata, created_at, updated_at
               FROM turns WHERE session_id = ? AND turn_id = ?"#,
        )
        .bind(session_id)
        .bind(turn_id)
        .fetch_optional(&self.pool)
        .await?)
    }

    pub async fn list_turn_event_enrichment_rows(
        &self,
        session_id: &str,
        turn_id: &str,
    ) -> Result<Vec<TurnEventEnrichmentRow>> {
        Ok(sqlx::query_as::<_, TurnEventEnrichmentRow>(
            r#"SELECT event_id, event_type, occurred_at, payload
               FROM events WHERE session_id = ? AND turn_id = ? ORDER BY rowid"#,
        )
        .bind(session_id)
        .bind(turn_id)
        .fetch_all(&self.pool)
        .await?)
    }
}
