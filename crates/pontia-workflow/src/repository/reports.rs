use super::WorkflowState;
use crate::models::WorkflowEventRow;
use pontia_core::{Error, Result};
use serde_json::json;

impl WorkflowState {
    pub(crate) async fn request_termination(
        &self,
        node_id: &str,
        session_id: &str,
        runtime_id: &str,
        reason: &str,
    ) -> Result<()> {
        let snapshot = self
            .queries
            .verify_active_turn(session_id, runtime_id)
            .await?;
        let turn = snapshot
            .turns
            .iter()
            .find(|turn| matches!(turn.state.as_str(), "queued" | "running"))
            .expect("verified active Turn");
        let event_id = format!("workflow-termination:{node_id}:{}", turn.turn_id);
        let payload = json!({"reason": reason, "session_id": session_id, "node_id": node_id, "turn_id": turn.turn_id, "runtime_id":runtime_id});
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let workflow_id: String = sqlx::query_scalar(
            "SELECT n.workflow_id FROM workflow_nodes n JOIN workflows w USING(workflow_id)
             WHERE n.node_id=? AND n.session_id=? AND n.submitted_at IS NULL AND w.state='running'",
        )
        .bind(node_id)
        .bind(session_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| {
            Error::StateConflict("Termination requires an active, unsubmitted node".into())
        })?;
        sqlx::query(
            "INSERT INTO workflow_events(event_id,workflow_id,sequence,event_type,payload)
                     SELECT ?,?,COALESCE(MAX(sequence),0)+1,'workflow.termination_requested',?
                     FROM workflow_events WHERE workflow_id=? ON CONFLICT(event_id) DO NOTHING",
        )
        .bind(event_id)
        .bind(&workflow_id)
        .bind(payload.to_string())
        .bind(&workflow_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        self.changed.notify_one();
        Ok(())
    }

    pub(crate) async fn termination_reason(&self, workflow_id: &str) -> Result<Option<String>> {
        Ok(sqlx::query_scalar("SELECT json_extract(payload,'$.reason') FROM workflow_events WHERE workflow_id=? AND event_type='workflow.termination_requested' ORDER BY sequence DESC LIMIT 1")
           .bind(workflow_id).fetch_optional(&self.pool).await?)
    }

    pub(crate) async fn pending_reports(&self, workflow_id: &str) -> Result<Vec<WorkflowEventRow>> {
        Ok(sqlx::query_as("SELECT e.* FROM workflow_events e JOIN workflows w USING(workflow_id)
             WHERE e.workflow_id=? AND e.sequence>w.last_report_sequence
               AND e.event_type IN ('workflow.termination_requested','workflow.completed','workflow.failed','workflow.idle')
             ORDER BY e.sequence")
           .bind(workflow_id).fetch_all(&self.pool).await?)
    }

    pub(crate) async fn mark_report_queued(&self, workflow_id: &str, sequence: i64) -> Result<()> {
        sqlx::query("UPDATE workflows SET last_report_sequence=MAX(last_report_sequence,?) WHERE workflow_id=?")
            .bind(sequence).bind(workflow_id).execute(&self.pool).await?;
        Ok(())
    }
}
