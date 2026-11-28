use super::SqliteEventRepository;
use pontia_core::{
    Result,
    domain::{EventType, TurnState},
};
use serde_json::Value;

#[derive(Debug, sqlx::FromRow)]
pub struct ReportEvent {
    pub turn_id: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
pub struct CompletedTurnWithBoundaries {
    pub turn_id: String,
    pub state: TurnState,
    pub input_summary: Option<String>,
    pub output_summary: Option<String>,
    pub failure_message: Option<String>,
    pub start_cursor: String,
    pub end_cursor: String,
}

impl SqliteEventRepository {
    pub async fn find_by_report_key(
        &self,
        session: &str,
        kind: EventType,
        key: &str,
    ) -> Result<Option<ReportEvent>> {
        Ok(sqlx::query_as(
            "SELECT turn_id FROM events WHERE session_id=? AND event_type=? AND report_key=?",
        )
        .bind(session)
        .bind(kind.to_string())
        .bind(key)
        .fetch_optional(&self.pool)
        .await?)
    }

    pub async fn find_turn_by_report_key(
        &self,
        session: &str,
        kind: EventType,
        key: &str,
    ) -> Result<Option<(String, TurnState)>> {
        Ok(sqlx::query_as(
            "SELECT t.turn_id,t.state FROM events e JOIN turns t USING(session_id,turn_id)
             WHERE e.session_id=? AND e.event_type=? AND e.report_key=?",
        )
        .bind(session)
        .bind(kind.to_string())
        .bind(key)
        .fetch_optional(&self.pool)
        .await?)
    }

    pub async fn latest_payload_by_type(
        &self,
        session: &str,
        kind: EventType,
    ) -> Result<Option<Value>> {
        Ok(sqlx::query_scalar(
            "SELECT payload FROM events WHERE session_id=? AND event_type=? ORDER BY rowid DESC LIMIT 1",
        )
            .bind(session)
            .bind(kind.to_string())
            .fetch_optional(&self.pool)
            .await?)
    }

    pub async fn list_payloads_by_type(
        &self,
        session: &str,
        kind: EventType,
        turn: Option<&str>,
    ) -> Result<Vec<Value>> {
        Ok(sqlx::query_scalar(
            "SELECT payload FROM events WHERE session_id=? AND event_type=? AND (?3 IS NULL OR turn_id=?3) ORDER BY rowid",
        )
            .bind(session)
            .bind(kind.to_string())
            .bind(turn)
            .fetch_all(&self.pool)
            .await?)
    }

    pub async fn list_turn_payloads_by_type(
        &self,
        session: &str,
        kind: EventType,
    ) -> Result<Vec<(String, Value)>> {
        Ok(sqlx::query_as(
            "SELECT turn_id,payload FROM events WHERE session_id=? AND event_type=? AND turn_id IS NOT NULL ORDER BY rowid",
        )
            .bind(session)
            .bind(kind.to_string())
            .fetch_all(&self.pool)
            .await?)
    }

    pub async fn latest_failure_after(
        &self,
        session: &str,
        after: EventType,
        failures: &[EventType],
    ) -> Result<Option<String>> {
        Ok(sqlx::query_scalar(
            "SELECT payload ->> '$.failure.message' FROM events
             WHERE session_id=?1 AND rowid>COALESCE((SELECT MAX(rowid) FROM events WHERE session_id=?1 AND event_type=?2),0)
             AND event_type IN (SELECT value FROM json_each(?3)) AND payload ->> '$.failure.message' IS NOT NULL
             ORDER BY rowid DESC LIMIT 1",
        )
            .bind(session)
            .bind(after.to_string())
            .bind(serde_json::to_string(failures)?)
            .fetch_optional(&self.pool)
            .await?)
    }

    pub async fn active_turn_started_payload(
        &self,
        session: &str,
    ) -> Result<Option<(String, Value)>> {
        Ok(sqlx::query_as(
            "SELECT e.turn_id,e.payload FROM events e JOIN turns t USING(session_id,turn_id)
             WHERE t.session_id=? AND t.state IN ('queued','running') AND e.event_type='turn.started' ORDER BY e.rowid DESC LIMIT 1",
        )
            .bind(session)
            .fetch_optional(&self.pool)
            .await?)
    }

    pub async fn completed_turns_before_with_boundaries(
        &self,
        session: &str,
        target: &str,
    ) -> Result<Vec<CompletedTurnWithBoundaries>> {
        Ok(sqlx::query_as(
            "SELECT t.turn_id,t.input_summary,t.output_summary,t.state,
                (SELECT e.payload ->> '$.failure.message' FROM events e
                 WHERE e.session_id=t.session_id AND e.turn_id=t.turn_id
                   AND e.event_type IN ('turn.completed','turn.failed','turn.interrupted')
                 ORDER BY e.rowid DESC LIMIT 1) AS failure_message,
                start.timeline_boundary ->> '$.cursor' AS start_cursor,
                finish.timeline_boundary ->> '$.cursor' AS end_cursor
             FROM turns t JOIN turns target ON target.session_id=t.session_id AND target.turn_id=?2
             JOIN events start ON start.rowid=(SELECT MIN(rowid) FROM events
                 WHERE session_id=t.session_id AND turn_id=t.turn_id AND event_type='turn.started' AND timeline_boundary IS NOT NULL)
             JOIN events finish ON finish.rowid=(SELECT MAX(rowid) FROM events
                 WHERE session_id=t.session_id AND turn_id=t.turn_id AND event_type IN ('turn.completed','turn.failed','turn.interrupted') AND timeline_boundary IS NOT NULL)
             WHERE t.session_id=?1 AND t.rowid<target.rowid AND t.state IN ('completed','failed','interrupted')
             ORDER BY t.rowid",
        )
            .bind(session)
            .bind(target)
            .fetch_all(&self.pool)
            .await?)
    }
}
