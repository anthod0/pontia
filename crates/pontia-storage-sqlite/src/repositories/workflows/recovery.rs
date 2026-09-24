use super::SqliteWorkflowRepository;
use crate::models::workflows::WorkflowRecoveryRow;
use pontia_core::{Error, Result};
use serde_json::json;
use sqlx::{Sqlite, Transaction};

#[derive(Debug, sqlx::FromRow)]
pub struct WorkflowRecoveryCandidate {
    pub failure_event_id: String,
    pub exit_event_id: String,
    pub node_id: String,
    pub session_id: String,
}

const CANDIDATE: &str = r#"
SELECT f.event_id AS failure_event_id, e.event_id AS exit_event_id, n.node_id, n.session_id
FROM workflows w
JOIN workflow_events f ON f.workflow_id=w.workflow_id AND f.event_type='workflow.failed'
    AND f.sequence=(SELECT MAX(sequence) FROM workflow_events WHERE workflow_id=w.workflow_id AND event_type='workflow.failed')
JOIN workflow_nodes n ON n.workflow_id=w.workflow_id AND n.submitted_at IS NULL AND n.session_id IS NOT NULL
    AND n.introduced_revision<=w.current_revision AND (n.retired_revision IS NULL OR n.retired_revision>w.current_revision)
JOIN sessions s ON s.session_id=n.session_id AND s.client_type='pi'
JOIN agent_bindings b ON b.session_id=s.session_id AND b.client_type='pi'
LEFT JOIN workflow_recoveries previous ON previous.recovery_id=json_extract(f.payload,'$.recovery_id')
JOIN events e ON e.session_id=s.session_id AND e.event_type='session.exited'
    AND e.source IN ('agent_client','runtime_manager')
    AND e.event_id=COALESCE(json_extract(f.payload,'$.cause_event_id'),previous.exit_event_id,
        (SELECT event_id FROM events WHERE session_id=s.session_id AND event_type='session.exited'
         AND source IN ('agent_client','runtime_manager') AND created_at<=f.created_at ORDER BY rowid DESC LIMIT 1))
WHERE w.workflow_id=? AND w.state='failed' AND w.activating_node_id IS NULL
    AND (s.state='exited' OR (s.state='idle' AND previous.state='failed'))
    AND (json_extract(f.payload,'$.cause_event_id')=e.event_id
         OR previous.state='failed'
         OR json_extract(f.payload,'$.failure_message')='Agent Client reported session.exited before Agent Node '||n.node_id||' Submission')
    AND (json_extract(f.payload,'$.node_id') IS NULL OR json_extract(f.payload,'$.node_id')=n.node_id)
    AND json_extract(e.payload,'$.runtime_instance_id') IS NOT NULL
    AND NOT EXISTS (SELECT 1 FROM workflow_nodes child WHERE child.workflow_id=w.workflow_id
        AND child.parent_node_id=n.node_id AND child.session_id IS NOT NULL
        AND child.introduced_revision<=w.current_revision AND (child.retired_revision IS NULL OR child.retired_revision>w.current_revision))
    AND NOT EXISTS (SELECT 1 FROM workflow_recoveries WHERE workflow_id=w.workflow_id AND state IN ('requested','preparing','dispatching'))
    AND NOT EXISTS (SELECT 1 FROM workflow_patches WHERE workflow_id=w.workflow_id AND state IN ('requested','planning'))
    AND NOT EXISTS (SELECT 1 FROM inbox_messages i WHERE i.session_id=s.session_id AND
        (i.state IN ('pending','resuming','dispatching','unknown') OR (i.state='dispatched' AND i.turn_id IS NULL)))
"#;

impl SqliteWorkflowRepository {
    pub async fn recovery_runtime_available(&self, id: &str) -> Result<bool> {
        Ok(sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM workflow_recoveries a JOIN sessions s ON s.session_id=a.session_id JOIN runtime_bindings r ON r.session_id=s.session_id WHERE a.recovery_id=? AND s.state IN ('idle','busy','interrupted') AND r.binding_state='confirmed' AND r.runtime_instance_id=a.runtime_instance_id)")
            .bind(id).fetch_one(&self.pool).await?)
    }

    pub async fn recovery_candidate(
        &self,
        workflow_id: &str,
    ) -> Result<Option<WorkflowRecoveryCandidate>> {
        Ok(sqlx::query_as(CANDIDATE)
            .bind(workflow_id)
            .fetch_optional(&self.pool)
            .await?)
    }

    pub async fn list_recoveries(&self, workflow_id: &str) -> Result<Vec<WorkflowRecoveryRow>> {
        Ok(
            sqlx::query_as("SELECT * FROM workflow_recoveries WHERE workflow_id=? ORDER BY rowid")
                .bind(workflow_id)
                .fetch_all(&self.pool)
                .await?,
        )
    }

    pub async fn request_recovery(
        &self,
        workflow_id: &str,
        failure_event_id: &str,
        recovery_id: &str,
    ) -> Result<WorkflowRecoveryRow> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        if let Some(existing) = sqlx::query_as::<_, WorkflowRecoveryRow>(
            "SELECT * FROM workflow_recoveries WHERE workflow_id=? AND failure_event_id=?",
        )
        .bind(workflow_id)
        .bind(failure_event_id)
        .fetch_optional(&mut *tx)
        .await?
        {
            return Ok(existing);
        }
        let candidate: WorkflowRecoveryCandidate = sqlx::query_as(CANDIDATE).bind(workflow_id).fetch_optional(&mut *tx).await?
            .ok_or_else(|| Error::StateConflict("Retry requires an unsubmitted Pi node with a confirmed exit, a native Session binding, and no pending or uncertain input".into()))?;
        if candidate.failure_event_id != failure_event_id {
            return Err(Error::StateConflict(
                "Workflow failure changed; reload before retrying".into(),
            ));
        }
        sqlx::query("INSERT INTO workflow_recoveries(recovery_id,workflow_id,failure_event_id,exit_event_id,node_id,session_id,message_id,state) VALUES (?,?,?,?,?,?,?,'requested')")
            .bind(recovery_id).bind(workflow_id).bind(failure_event_id).bind(&candidate.exit_event_id)
            .bind(&candidate.node_id).bind(&candidate.session_id).bind(format!("input_{recovery_id}"))
            .execute(&mut *tx).await?;
        sqlx::query("UPDATE workflows SET state='recovering',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE workflow_id=?")
            .bind(workflow_id).execute(&mut *tx).await?;
        let row: WorkflowRecoveryRow =
            sqlx::query_as("SELECT * FROM workflow_recoveries WHERE recovery_id=?")
                .bind(recovery_id)
                .fetch_one(&mut *tx)
                .await?;
        recovery_event(&mut tx, &row, "workflow.recovery_requested", None).await?;
        tx.commit().await?;
        Ok(row)
    }

    pub async fn claim_recovery_preparation(&self, id: &str) -> Result<bool> {
        Ok(sqlx::query("UPDATE workflow_recoveries SET state='preparing' WHERE recovery_id=? AND state='requested'")
            .bind(id).execute(&self.pool).await?.rows_affected()==1)
    }

    pub async fn start_recovery_delivery(&self, id: &str) -> Result<()> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let result=sqlx::query("UPDATE workflow_recoveries SET state='dispatching',runtime_instance_id=(SELECT runtime_instance_id FROM runtime_bindings WHERE session_id=workflow_recoveries.session_id AND binding_state='confirmed'),updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE recovery_id=? AND state='preparing' AND EXISTS (SELECT 1 FROM sessions s JOIN runtime_bindings r ON r.session_id=s.session_id WHERE s.session_id=workflow_recoveries.session_id AND s.state='idle' AND r.binding_state='confirmed' AND r.runtime_instance_id IS NOT NULL) AND EXISTS (SELECT 1 FROM inbox_messages WHERE message_id=workflow_recoveries.message_id AND state='resuming')")
            .bind(id).execute(&mut *tx).await?;
        if result.rows_affected() != 1 {
            return Err(Error::StateConflict(
                "Recovered Session is not ready for input".into(),
            ));
        }
        let row: WorkflowRecoveryRow =
            sqlx::query_as("SELECT * FROM workflow_recoveries WHERE recovery_id=?")
                .bind(id)
                .fetch_one(&mut *tx)
                .await?;
        sqlx::query("UPDATE workflows SET state='running',failure_message=NULL,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE workflow_id=? AND state='recovering'")
            .bind(&row.workflow_id).execute(&mut *tx).await?;
        recovery_event(&mut tx, &row, "workflow.recovery_ready", None).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn finish_recovery(&self, id: &str, failure: Option<&str>) -> Result<()> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let changed=sqlx::query("UPDATE workflow_recoveries SET state=?,failure_message=?,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE recovery_id=? AND state IN ('requested','preparing','dispatching')")
            .bind(if failure.is_some(){"failed"}else{"completed"}).bind(failure).bind(id).execute(&mut *tx).await?.rows_affected();
        if changed == 0 {
            return Ok(());
        }
        let row: WorkflowRecoveryRow =
            sqlx::query_as("SELECT * FROM workflow_recoveries WHERE recovery_id=?")
                .bind(id)
                .fetch_one(&mut *tx)
                .await?;
        if let Some(reason) = failure {
            sqlx::query("UPDATE workflows SET state='failed',failure_message=?,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE workflow_id=? AND state IN ('recovering','running','paused')")
                .bind(reason).bind(&row.workflow_id).execute(&mut *tx).await?;
            recovery_event(&mut tx, &row, "workflow.failed", failure).await?;
        } else {
            recovery_event(&mut tx, &row, "workflow.recovery_delivered", None).await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn recover_interrupted_preparations(&self) -> Result<()> {
        let ids: Vec<String> = sqlx::query_scalar(
            "SELECT recovery_id FROM workflow_recoveries WHERE state='preparing'",
        )
        .fetch_all(&self.pool)
        .await?;
        for id in ids {
            self.finish_recovery(&id,Some("Workflow recovery preparation was interrupted by service restart; inspect the Session and retry explicitly")).await?;
        }
        Ok(())
    }

    pub async fn recovered_node_runtime(&self, node_id: &str) -> Result<Option<String>> {
        Ok(sqlx::query_scalar("SELECT runtime_instance_id FROM workflow_recoveries WHERE node_id=? AND runtime_instance_id IS NOT NULL ORDER BY rowid DESC LIMIT 1")
            .bind(node_id).fetch_optional(&self.pool).await?)
    }
}

async fn recovery_event(
    tx: &mut Transaction<'_, Sqlite>,
    row: &WorkflowRecoveryRow,
    kind: &str,
    failure: Option<&str>,
) -> Result<()> {
    let payload = json!({"recovery_id":row.recovery_id,"failure_event_id":row.failure_event_id,"cause_event_id":row.exit_event_id,"node_id":row.node_id,"session_id":row.session_id,"message_id":row.message_id,"runtime_instance_id":row.runtime_instance_id,"failure_message":failure});
    sqlx::query("INSERT INTO workflow_events(event_id,workflow_id,sequence,event_type,payload) SELECT ?,?,COALESCE(MAX(sequence),0)+1,?,? FROM workflow_events WHERE workflow_id=?")
        .bind(format!("{}:{kind}",row.recovery_id)).bind(&row.workflow_id).bind(kind).bind(payload.to_string()).bind(&row.workflow_id).execute(&mut **tx).await?;
    Ok(())
}
