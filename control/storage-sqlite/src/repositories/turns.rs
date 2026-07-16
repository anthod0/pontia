use sqlx::{Sqlite, SqlitePool, Transaction};

use crate::models::turns::{TurnEventEnrichmentRow, TurnProjectionRow, TurnRow};

use pontia_core::Result;

const LOAD_TURN_PROJECTIONS_SQL: &str = "SELECT turn_id, session_id, turn_index, head_cursor, tail_cursor, parent_turn_id, topology_status, state, state_version, input_summary, output_summary, metadata FROM turns WHERE session_id = ?";

#[derive(Debug, Clone)]
pub struct TurnProjectionUpsertRecord {
    pub turn_id: String,
    pub session_id: String,
    pub turn_index: i64,
    pub head_cursor: Option<String>,
    pub tail_cursor: Option<String>,
    pub parent_turn_id: Option<String>,
    pub topology_status: String,
    pub state: String,
    pub state_version: i64,
    pub input_summary: Option<String>,
    pub output_summary: Option<String>,
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
        Ok(
            sqlx::query_as::<_, TurnProjectionRow>(LOAD_TURN_PROJECTIONS_SQL)
                .bind(session_id)
                .fetch_all(&self.pool)
                .await?,
        )
    }

    pub async fn load_projection_rows_in_tx(
        tx: &mut Transaction<'_, Sqlite>,
        session_id: &str,
    ) -> Result<Vec<TurnProjectionRow>> {
        Ok(
            sqlx::query_as::<_, TurnProjectionRow>(LOAD_TURN_PROJECTIONS_SQL)
                .bind(session_id)
                .fetch_all(&mut **tx)
                .await?,
        )
    }

    pub async fn serialize_session_turn_writes_in_tx(
        tx: &mut Transaction<'_, Sqlite>,
        session_id: &str,
    ) -> Result<()> {
        let updated = sqlx::query(
            "UPDATE sessions SET next_turn_index = next_turn_index WHERE session_id = ?",
        )
        .bind(session_id)
        .execute(&mut **tx)
        .await?
        .rows_affected();
        if updated != 1 {
            return Err(pontia_core::Error::Domain(format!(
                "cannot allocate turn_index for unknown session {session_id}"
            )));
        }
        Ok(())
    }

    pub async fn turn_index_in_tx(
        tx: &mut Transaction<'_, Sqlite>,
        session_id: &str,
        turn_id: &str,
    ) -> Result<Option<i64>> {
        Ok(
            sqlx::query_scalar("SELECT turn_index FROM turns WHERE session_id = ? AND turn_id = ?")
                .bind(session_id)
                .bind(turn_id)
                .fetch_optional(&mut **tx)
                .await?,
        )
    }

    pub async fn allocate_turn_index_in_tx(
        tx: &mut Transaction<'_, Sqlite>,
        session_id: &str,
    ) -> Result<i64> {
        Ok(sqlx::query_scalar(
            r#"UPDATE sessions
               SET next_turn_index = next_turn_index + 1
               WHERE session_id = ?
               RETURNING next_turn_index - 1"#,
        )
        .bind(session_id)
        .fetch_one(&mut **tx)
        .await?)
    }

    pub async fn get_projection(&self, turn_id: &str) -> Result<Option<TurnProjectionRow>> {
        Ok(sqlx::query_as::<_, TurnProjectionRow>(
            "SELECT turn_id, session_id, turn_index, head_cursor, tail_cursor, parent_turn_id, topology_status, state, state_version, input_summary, output_summary, metadata FROM turns WHERE turn_id = ?",
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
               (turn_id, session_id, turn_index, head_cursor, tail_cursor, parent_turn_id, topology_status, state, state_version,
                input_summary, output_summary, metadata)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
               ON CONFLICT(turn_id) DO UPDATE SET
                   head_cursor = excluded.head_cursor,
                   tail_cursor = excluded.tail_cursor,
                   parent_turn_id = excluded.parent_turn_id,
                   topology_status = excluded.topology_status,
                   state = excluded.state,
                   state_version = excluded.state_version,
                   input_summary = excluded.input_summary,
                   output_summary = excluded.output_summary,
                   metadata = excluded.metadata,
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')"#,
        )
        .bind(turn.turn_id)
        .bind(turn.session_id)
        .bind(turn.turn_index)
        .bind(turn.head_cursor)
        .bind(turn.tail_cursor)
        .bind(turn.parent_turn_id)
        .bind(turn.topology_status)
        .bind(turn.state)
        .bind(turn.state_version)
        .bind(turn.input_summary)
        .bind(turn.output_summary)
        .bind(turn.metadata)
        .execute(&mut **tx)
        .await?;

        Ok(())
    }

    pub async fn list_turns(&self, session_id: &str) -> Result<Vec<TurnRow>> {
        Ok(sqlx::query_as::<_, TurnRow>(
            r#"SELECT turn_id, session_id, turn_index, head_cursor, tail_cursor, parent_turn_id, topology_status, state, input_summary, output_summary,
                      failure_message, metadata, created_at, updated_at
               FROM turns WHERE session_id = ? ORDER BY turn_index"#,
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn get_turn(&self, session_id: &str, turn_id: &str) -> Result<Option<TurnRow>> {
        Ok(sqlx::query_as::<_, TurnRow>(
            r#"SELECT turn_id, session_id, turn_index, head_cursor, tail_cursor, parent_turn_id, topology_status, state, input_summary, output_summary,
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
