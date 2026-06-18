use sqlx::SqlitePool;

use crate::models::turns::{TurnEventEnrichmentRow, TurnRow};

use pontia_core::Result;

#[derive(Debug, Clone)]
pub struct SqliteTurnRepository {
    pool: SqlitePool,
}

impl SqliteTurnRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
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
