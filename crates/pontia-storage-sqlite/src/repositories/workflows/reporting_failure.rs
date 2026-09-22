use pontia_core::Result;
use serde_json::json;

use super::SqliteWorkflowRepository;

impl SqliteWorkflowRepository {
    /// Atomically fails the still-current node (including submitted/exiting)
    /// from a persisted integration error for its current confirmed runtime.
    pub async fn fail_for_turn_start_reporting(
        &self,
        workflow_id: &str,
        node_id: &str,
        event_id: &str,
    ) -> Result<bool> {
        // Normal reconciliation is read-only. Recheck under the write lock
        // below only when there is a candidate reporting failure.
        let has_failure: bool = sqlx::query_scalar(
            r#"SELECT EXISTS (
                 SELECT 1 FROM events e
                 JOIN workflow_nodes n ON n.session_id = e.session_id
                 JOIN runtime_bindings r ON r.session_id = n.session_id
                 WHERE n.workflow_id = ? AND n.node_id = ?
                   AND e.event_type = 'session.error' AND e.source = 'runtime_manager'
                   AND json_extract(e.payload, '$.reason') = 'turn_start_reporting_failed'
                   AND json_extract(e.payload, '$.runtime_instance_id') = r.runtime_instance_id
                   AND r.binding_state = 'confirmed'
               )"#,
        )
        .bind(workflow_id)
        .bind(node_id)
        .fetch_one(&self.pool)
        .await?;
        if !has_failure {
            return Ok(false);
        }
        let mut tx = self.pool.begin().await?;
        let locked = sqlx::query("UPDATE workflows SET workflow_id = workflow_id WHERE workflow_id = ? AND state = 'running'")
            .bind(workflow_id).execute(&mut *tx).await?;
        if locked.rows_affected() == 0 {
            return Ok(false);
        }
        let failure: Option<(String, String, String)> = sqlx::query_as(
            r#"SELECT json_extract(e.payload, '$.failure.message'), n.session_id, r.runtime_instance_id
               FROM workflow_nodes n
               JOIN workflows w ON w.workflow_id = n.workflow_id
               JOIN runtime_bindings r ON r.session_id = n.session_id
               JOIN events e ON e.session_id = n.session_id
               WHERE n.workflow_id = ? AND n.node_id = ?
                 AND n.introduced_revision <= w.current_revision
                 AND (n.retired_revision IS NULL OR n.retired_revision > w.current_revision)
                 AND e.event_type = 'session.error' AND e.source = 'runtime_manager'
                 AND json_extract(e.payload, '$.reason') = 'turn_start_reporting_failed'
                 AND json_extract(e.payload, '$.runtime_instance_id') = r.runtime_instance_id
                 AND r.binding_state = 'confirmed'
                 AND NOT EXISTS (
                     SELECT 1 FROM workflow_nodes child
                     WHERE child.parent_node_id = n.node_id AND child.session_id IS NOT NULL
                       AND child.introduced_revision <= w.current_revision
                       AND (child.retired_revision IS NULL OR child.retired_revision > w.current_revision)
                 )
               ORDER BY e.rowid DESC LIMIT 1"#,
        ).bind(workflow_id).bind(node_id).fetch_optional(&mut *tx).await?;
        let Some((message, session_id, runtime_instance_id)) = failure else {
            tx.commit().await?;
            return Ok(false);
        };
        sqlx::query("UPDATE workflows SET state = 'failed', failure_message = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE workflow_id = ?")
            .bind(&message).bind(workflow_id).execute(&mut *tx).await?;
        let payload = json!({
            "node_id": node_id,
            "session_id": session_id,
            "runtime_instance_id": runtime_instance_id,
            "failure_message": message,
            "reason": "turn_start_reporting_failed",
        });
        sqlx::query(
            r#"INSERT INTO workflow_events (event_id, workflow_id, sequence, event_type, payload)
               SELECT ?, ?, COALESCE(MAX(sequence), 0) + 1, 'workflow.failed', ?
               FROM workflow_events WHERE workflow_id = ?"#,
        )
        .bind(event_id)
        .bind(workflow_id)
        .bind(payload.to_string())
        .bind(workflow_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(true)
    }

    pub async fn failure_node_id(&self, workflow_id: &str) -> Result<Option<String>> {
        Ok(sqlx::query_scalar::<_, Option<String>>(
            "SELECT json_extract(payload, '$.node_id') FROM workflow_events WHERE workflow_id = ? AND event_type = 'workflow.failed' ORDER BY sequence DESC LIMIT 1",
        ).bind(workflow_id).fetch_optional(&self.pool).await?.flatten())
    }
}
