use super::SqliteWorkflowRepository;
use crate::models::WorkflowTerminalEventRow;
use pontia_core::Result;

// Follow-up Turn facts identify a canonical Turn, not a Runtime. Correlate them
// only through its preceding client-confirmed start; never through today's
// runtime binding or a terminal payload supplied outside that contract.
// Keep this read model shared by selection and atomic Patch predicates.
pub(crate) const WORKFLOW_TERMINAL_EVENTS: &str = r#"
WITH workflow_terminal_events AS (
    SELECT e.event_id, e.session_id, e.turn_id, e.event_type, e.position AS event_order,
           CASE WHEN e.event_type = 'session.exited'
                THEN json_extract(e.payload, '$.runtime_id')
                ELSE (
                    SELECT json_extract(started.payload, '$.runtime_id')
                    FROM workflow_agent_events AS started
                    WHERE started.session_id = e.session_id
                      AND started.turn_id = e.turn_id
                      AND started.event_type = 'turn.started'
                      AND started.source IN ('agent_adapter', 'agent_client')
                      AND started.position < e.position
                    ORDER BY started.position
                    LIMIT 1
                )
           END AS runtime_id
    FROM workflow_agent_events AS e
    WHERE (e.source IN ('agent_adapter', 'agent_client')
           AND e.event_type IN ('turn.completed', 'turn.failed', 'turn.interrupted')
           AND NOT EXISTS (SELECT 1 FROM workflow_agent_events startup
               WHERE startup.session_id=e.session_id
                 AND startup.event_type IN ('session.starting','session.resuming')
                 AND startup.position > COALESCE((SELECT MIN(started.position) FROM workflow_agent_events started
                     WHERE started.session_id=e.session_id AND started.turn_id=e.turn_id
                       AND started.event_type='turn.started'
                       AND started.source IN ('agent_adapter','agent_client')), e.position)))
       OR (e.source IN ('agent_client', 'runtime_manager')
           AND e.event_type = 'session.exited'
           AND NOT EXISTS (SELECT 1 FROM workflow_agent_events startup
               WHERE startup.session_id=e.session_id
                 AND startup.event_type IN ('session.starting','session.resuming')
                 AND startup.position > e.position))
)
"#;

impl SqliteWorkflowRepository {
    pub async fn latest_workflow_terminal_event(
        &self,
        session_id: &str,
        runtime_id: Option<&str>,
        turn_id: Option<&str>,
    ) -> Result<Option<WorkflowTerminalEventRow>> {
        self.synchronize().await?;
        Ok(sqlx::query_as::<_, WorkflowTerminalEventRow>(&format!(
            r#"{WORKFLOW_TERMINAL_EVENTS}
               SELECT event_id, turn_id, event_type, runtime_id
               FROM workflow_terminal_events
               WHERE session_id = ?
                 AND (? IS NULL OR runtime_id = ?)
                 AND (? IS NULL OR turn_id = ? OR event_type = 'session.exited')
               ORDER BY CASE event_type WHEN 'session.exited' THEN 0 ELSE 1 END, event_order DESC
               LIMIT 1"#,
        ))
        .bind(session_id)
        .bind(runtime_id)
        .bind(runtime_id)
        .bind(turn_id)
        .bind(turn_id)
        .fetch_optional(&self.pool)
        .await?)
    }
}
