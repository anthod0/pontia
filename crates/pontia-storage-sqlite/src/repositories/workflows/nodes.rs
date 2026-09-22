use pontia_core::{Error, Result};

use super::SqliteWorkflowRepository;

impl SqliteWorkflowRepository {
    pub async fn bind_node_session(&self, node_id: &str, session_id: &str) -> Result<()> {
        let result = sqlx::query(
            r#"UPDATE workflow_nodes
               SET session_id = ?
               WHERE node_id = ?
                 AND session_id IS NULL
                 AND EXISTS (
                     SELECT 1 FROM workflows
                     WHERE workflows.workflow_id = workflow_nodes.workflow_id
                       AND workflow_nodes.introduced_revision <= workflows.current_revision
                       AND (workflow_nodes.retired_revision IS NULL
                            OR workflow_nodes.retired_revision > workflows.current_revision)
                 )"#,
        )
        .bind(session_id)
        .bind(node_id)
        .execute(&self.pool)
        .await?;
        if result.rows_affected() != 1 {
            return Err(Error::StateConflict(format!(
                "workflow node {node_id} is missing or already has a session"
            )));
        }
        Ok(())
    }

    pub async fn claim_node_activation(
        &self,
        workflow_id: &str,
        node_id: &str,
        event_id: &str,
    ) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        let result = sqlx::query(
            r#"UPDATE workflows
               SET activating_node_id = ?
               WHERE workflow_id = ?
                 AND state = 'running'
                 AND activating_node_id IS NULL
                 AND EXISTS (
                     SELECT 1 FROM workflow_nodes
                     WHERE workflow_nodes.workflow_id = workflows.workflow_id
                       AND workflow_nodes.node_id = ?
                       AND workflow_nodes.session_id IS NULL
                       AND workflow_nodes.introduced_revision <= workflows.current_revision
                       AND (workflow_nodes.retired_revision IS NULL
                            OR workflow_nodes.retired_revision > workflows.current_revision)
                 )"#,
        )
        .bind(node_id)
        .bind(workflow_id)
        .bind(node_id)
        .execute(&mut *tx)
        .await?;
        if result.rows_affected() != 1 {
            return Err(Error::StateConflict(format!(
                "workflow {workflow_id} must be running without another node activation"
            )));
        }
        let sequence: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(sequence), 0) + 1 FROM workflow_events WHERE workflow_id = ?",
        )
        .bind(workflow_id)
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query(
            r#"INSERT INTO workflow_events
               (event_id, workflow_id, sequence, event_type, payload)
               VALUES (?, ?, ?, 'workflow.node_activation_requested', ?)"#,
        )
        .bind(event_id)
        .bind(workflow_id)
        .bind(sequence)
        .bind(serde_json::json!({ "node_id": node_id }).to_string())
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn finish_node_activation(&self, node_id: &str, session_id: &str) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        let node_result = sqlx::query(
            r#"UPDATE workflow_nodes
               SET session_id = ?
               WHERE node_id = ?
                 AND session_id IS NULL
                 AND EXISTS (
                     SELECT 1 FROM workflows
                     WHERE workflows.workflow_id = workflow_nodes.workflow_id
                       AND workflows.state = 'running'
                       AND workflows.activating_node_id = workflow_nodes.node_id
                       AND workflow_nodes.introduced_revision <= workflows.current_revision
                       AND (workflow_nodes.retired_revision IS NULL
                            OR workflow_nodes.retired_revision > workflows.current_revision)
                 )"#,
        )
        .bind(session_id)
        .bind(node_id)
        .execute(&mut *tx)
        .await?;
        if node_result.rows_affected() != 1 {
            return Err(Error::StateConflict(format!(
                "workflow node {node_id} is not the claimed running activation"
            )));
        }
        let workflow_result = sqlx::query(
            r#"UPDATE workflows
               SET activating_node_id = NULL
               WHERE activating_node_id = ? AND state = 'running'"#,
        )
        .bind(node_id)
        .execute(&mut *tx)
        .await?;
        if workflow_result.rows_affected() != 1 {
            return Err(Error::StateConflict(format!(
                "workflow node {node_id} activation claim disappeared"
            )));
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn release_node_activation(&self, workflow_id: &str, node_id: &str) -> Result<()> {
        sqlx::query(
            "UPDATE workflows SET activating_node_id = NULL WHERE workflow_id = ? AND activating_node_id = ?",
        )
        .bind(workflow_id)
        .bind(node_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn record_node_submission(
        &self,
        node_id: &str,
        runtime_instance_id: &str,
        event_id: &str,
    ) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        let context: Option<(String, Option<String>, Option<String>)> = sqlx::query_as(
            r#"SELECT n.workflow_id, n.session_id, s.current_turn_id
               FROM workflow_nodes AS n
               LEFT JOIN sessions AS s ON s.session_id = n.session_id
               WHERE n.node_id = ?"#,
        )
        .bind(node_id)
        .fetch_optional(&mut *tx)
        .await?;
        let result = sqlx::query(
            r#"UPDATE workflow_nodes
               SET submitted_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                   submitted_runtime_instance_id = ?
               WHERE node_id = ?
                 AND submitted_at IS NULL
                 AND NOT EXISTS (
                     SELECT 1 FROM sessions
                     WHERE sessions.session_id = workflow_nodes.session_id
                       AND sessions.state = 'error'
                 )
                 AND EXISTS (
                     SELECT 1 FROM workflows
                     WHERE workflows.workflow_id = workflow_nodes.workflow_id
                       AND workflows.state = 'running'
                       AND workflow_nodes.introduced_revision <= workflows.current_revision
                       AND (workflow_nodes.retired_revision IS NULL
                            OR workflow_nodes.retired_revision > workflows.current_revision)
                 )"#,
        )
        .bind(runtime_instance_id)
        .bind(node_id)
        .execute(&mut *tx)
        .await?;
        if result.rows_affected() != 1 {
            return Err(Error::StateConflict(format!(
                "workflow node {node_id} must be unsubmitted in a running workflow without a Session error"
            )));
        }
        let (workflow_id, session_id, turn_id) = context.ok_or_else(|| {
            Error::Domain(format!("submitted Workflow Node {node_id} is missing"))
        })?;
        let sequence: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(sequence), 0) + 1 FROM workflow_events WHERE workflow_id = ?",
        )
        .bind(&workflow_id)
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query(
            r#"INSERT INTO workflow_events
               (event_id, workflow_id, sequence, event_type, payload)
               VALUES (?, ?, ?, 'workflow.node_submitted', ?)"#,
        )
        .bind(event_id)
        .bind(&workflow_id)
        .bind(sequence)
        .bind(
            serde_json::json!({
                "node_id": node_id,
                "session_id": session_id,
                "turn_id": turn_id,
                "runtime_instance_id": runtime_instance_id,
            })
            .to_string(),
        )
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn claim_node_exit_request(
        &self,
        node_id: &str,
        runtime_instance_id: &str,
    ) -> Result<bool> {
        let result = sqlx::query(
            r#"UPDATE workflow_nodes
               SET exit_request_started_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE node_id = ?
                 AND submitted_runtime_instance_id = ?
                 AND exit_request_started_at IS NULL
                 AND EXISTS (
                     SELECT 1 FROM workflows
                     WHERE workflows.workflow_id = workflow_nodes.workflow_id
                       AND workflows.state = 'running'
                       AND workflow_nodes.introduced_revision <= workflows.current_revision
                       AND (workflow_nodes.retired_revision IS NULL
                            OR workflow_nodes.retired_revision > workflows.current_revision)
                 )"#,
        )
        .bind(node_id)
        .bind(runtime_instance_id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }
}
