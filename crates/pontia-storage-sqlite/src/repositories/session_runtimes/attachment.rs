use super::SqliteSessionRuntimeRepository;
use pontia_core::Result;
use sqlx::{Sqlite, SqliteConnection, Transaction};

#[derive(sqlx::FromRow)]
pub struct ActiveAttachment {
    pub runtime_id: String,
    pub client_type: String,
    pub runtime_state: String,
    pub process_fingerprint: Option<String>,
}

impl SqliteSessionRuntimeRepository {
    pub async fn active_attachment(
        conn: &mut SqliteConnection,
        session: &str,
        role: &str,
    ) -> Result<Option<ActiveAttachment>> {
        let mut rows: Vec<ActiveAttachment> = sqlx::query_as(
            "SELECT r.runtime_id,s.client_type,r.state AS runtime_state,r.process_fingerprint
             FROM session_runtimes r JOIN sessions s USING(session_id)
             WHERE r.session_id=? AND r.role=? AND r.state IN ('starting','running')
               AND (s.state IN ('created','starting','idle','busy','waiting_for_interaction','interrupted')
                    OR (s.state='error' AND r.state='starting'))",
        )
             .bind(session)
             .bind(role)
             .fetch_all(conn)
             .await?;
        if rows.len() > 1 {
            return Err(pontia_core::Error::StateConflict(
                "Session has multiple active control runtimes".into(),
            ));
        }
        Ok(rows.pop())
    }

    pub async fn active_control_runtime_id(
        &self,
        session: &str,
        role: &str,
    ) -> Result<Option<String>> {
        let mut conn = self.pool.acquire().await?;
        Ok(Self::active_attachment(&mut conn, session, role)
            .await?
            .map(|row| row.runtime_id))
    }

    pub async fn backfill_process_fingerprint_in_tx(
        tx: &mut Transaction<'_, Sqlite>,
        session: &str,
        runtime: &str,
        fingerprint: &str,
    ) -> Result<()> {
        let result = sqlx::query(
            "UPDATE session_runtimes SET process_fingerprint=? WHERE session_id=? AND runtime_id=? AND state='starting' AND process_fingerprint IS NULL",
        )
            .bind(fingerprint)
            .bind(session)
            .bind(runtime)
            .execute(&mut **tx)
            .await?;
        if result.rows_affected() != 1 {
            return Err(pontia_core::Error::StateConflict(
                "Starting runtime did not accept its process fingerprint".into(),
            ));
        }
        Ok(())
    }

    pub async fn pending_ready_fingerprint(
        conn: &mut SqliteConnection,
        session: &str,
        runtime: &str,
        client: &str,
        key: &str,
    ) -> Result<Option<String>> {
        Ok(sqlx::query_scalar(
            "SELECT r.process_fingerprint FROM session_runtimes r JOIN sessions s USING(session_id)
            WHERE s.session_id=?1 AND r.runtime_id=?2 AND s.client_type=?3 AND s.state IN ('created','starting')
            AND r.state='starting' AND r.process_fingerprint IS NOT NULL AND NOT EXISTS (
                SELECT 1 FROM agent_bindings b WHERE b.client_type=?3 AND (b.session_id=?1 OR b.client_session_key=?4))",
        )
            .bind(session)
            .bind(runtime)
            .bind(client)
            .bind(key)
            .fetch_optional(conn)
            .await?)
    }

    pub async fn list_active_by_role(&self, role: &str) -> Result<Vec<(String, String)>> {
        Ok(sqlx::query_as(
            "SELECT session_id,runtime_id FROM session_runtimes WHERE role=? AND state IN ('starting','running') ORDER BY session_id,runtime_id",
        )
            .bind(role)
            .fetch_all(&self.pool)
            .await?)
    }

    pub async fn count_running_by_role(&self, session: &str, role: &str) -> Result<i64> {
        Ok(sqlx::query_scalar(
            "SELECT COUNT(*) FROM session_runtimes WHERE session_id=? AND role=? AND state='running'",
        )
            .bind(session)
            .bind(role)
            .fetch_one(&self.pool)
            .await?)
    }

    pub async fn session_exited_with_runtime(&self, session: &str, runtime: &str) -> Result<bool> {
        Ok(sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM sessions JOIN session_runtimes USING(session_id) WHERE session_id=? AND runtime_id=? AND sessions.state='exited')",
        )
            .bind(session)
            .bind(runtime)
            .fetch_one(&self.pool)
            .await?)
    }
}
