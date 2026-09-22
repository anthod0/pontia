use pontia_core::Result;

use crate::models::workflows::{WorkflowAgentEventRow, WorkflowEventRow};

use super::SqliteWorkflowRepository;

impl SqliteWorkflowRepository {
    pub async fn terminal_event_precedes_latest_resume(
        &self,
        workflow_id: &str,
        event_id: &str,
    ) -> Result<bool> {
        Ok(sqlx::query_scalar::<_, i64>(
            r#"SELECT EXISTS (
                   SELECT 1
                   FROM events AS agent_event
                   JOIN workflow_events AS resumed
                     ON resumed.workflow_id = ?
                    AND resumed.event_type = 'workflow.resumed'
                   WHERE agent_event.event_id = ?
                     AND resumed.sequence = (
                         SELECT MAX(sequence)
                         FROM workflow_events
                         WHERE workflow_id = ? AND event_type = 'workflow.resumed'
                     )
                     AND agent_event.created_at <= resumed.created_at
                     AND EXISTS (
                         SELECT 1
                         FROM workflow_events AS paused
                         WHERE paused.workflow_id = resumed.workflow_id
                           AND paused.event_type = 'workflow.paused'
                           AND paused.sequence < resumed.sequence
                     )
               )"#,
        )
        .bind(workflow_id)
        .bind(event_id)
        .bind(workflow_id)
        .fetch_one(&self.pool)
        .await?
            != 0)
    }

    pub async fn append_event(
        &self,
        event_id: &str,
        workflow_id: &str,
        event_type: &str,
        payload: &str,
    ) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        let sequence: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(sequence), 0) + 1 FROM workflow_events WHERE workflow_id = ?",
        )
        .bind(workflow_id)
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query(
            r#"INSERT INTO workflow_events
               (event_id, workflow_id, sequence, event_type, payload)
               VALUES (?, ?, ?, ?, ?)"#,
        )
        .bind(event_id)
        .bind(workflow_id)
        .bind(sequence)
        .bind(event_type)
        .bind(payload)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn list_events(&self, workflow_id: &str) -> Result<Vec<WorkflowEventRow>> {
        Ok(sqlx::query_as::<_, WorkflowEventRow>(
            r#"SELECT event_id, workflow_id, sequence, event_type, payload, created_at
               FROM workflow_events WHERE workflow_id = ? ORDER BY sequence"#,
        )
        .bind(workflow_id)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn list_workflow_agent_events(
        &self,
        workflow_id: &str,
    ) -> Result<Vec<WorkflowAgentEventRow>> {
        Ok(sqlx::query_as::<_, WorkflowAgentEventRow>(
            r#"SELECT e.rowid, e.event_id, e.session_id, e.turn_id, e.source, e.event_type,
                      e.occurred_at, e.payload, e.created_at
               FROM events AS e
               WHERE EXISTS (
                   SELECT 1 FROM workflow_nodes AS n
                   WHERE n.workflow_id = ? AND n.session_id = e.session_id
               ) OR EXISTS (
                   SELECT 1 FROM workflow_patches AS p
                   WHERE p.workflow_id = ?
                     AND (p.requesting_session_id = e.session_id
                          OR p.replanner_session_id = e.session_id)
               )
               ORDER BY e.rowid"#,
        )
        .bind(workflow_id)
        .bind(workflow_id)
        .fetch_all(&self.pool)
        .await?)
    }
}
