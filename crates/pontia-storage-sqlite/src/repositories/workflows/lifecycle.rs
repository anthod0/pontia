use pontia_core::{Error, Result};

use super::SqliteWorkflowRepository;

struct RunningWorkflowTransition<'a> {
    workflow_id: &'a str,
    unsubmitted_node_id: Option<&'a str>,
    event_id: &'a str,
    state: &'a str,
    event_type: &'a str,
    failure_message: Option<&'a str>,
    payload: &'a str,
}

impl SqliteWorkflowRepository {
    pub async fn start_workflow(&self, workflow_id: &str, event_id: &str) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        let result = sqlx::query(
            r#"UPDATE workflows
               SET state = 'running',
                   started_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE workflow_id = ? AND state = 'pending'"#,
        )
        .bind(workflow_id)
        .execute(&mut *tx)
        .await?;
        if result.rows_affected() != 1 {
            return Err(Error::StateConflict(format!(
                "workflow {workflow_id} must exist in pending state"
            )));
        }
        sqlx::query(
            r#"INSERT INTO workflow_events
               (event_id, workflow_id, sequence, event_type, payload)
               VALUES (?, ?, 1, 'workflow.started', '{}')"#,
        )
        .bind(event_id)
        .bind(workflow_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn pause_workflow(&self, workflow_id: &str, event_id: &str) -> Result<()> {
        self.transition_workflow_state(
            workflow_id,
            event_id,
            "running",
            "paused",
            "workflow.paused",
        )
        .await
    }

    pub async fn resume_workflow(&self, workflow_id: &str, event_id: &str) -> Result<()> {
        self.transition_workflow_state(
            workflow_id,
            event_id,
            "paused",
            "running",
            "workflow.resumed",
        )
        .await
    }

    async fn transition_workflow_state(
        &self,
        workflow_id: &str,
        event_id: &str,
        expected_state: &str,
        state: &str,
        event_type: &str,
    ) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        let result = sqlx::query(
            r#"UPDATE workflows
               SET state = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE workflow_id = ?
                 AND state = ?
                 AND activating_node_id IS NULL"#,
        )
        .bind(state)
        .bind(workflow_id)
        .bind(expected_state)
        .execute(&mut *tx)
        .await?;
        if result.rows_affected() != 1 {
            return Err(Error::StateConflict(format!(
                "workflow {workflow_id} must be {expected_state} without a node activation"
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
               VALUES (?, ?, ?, ?, '{}')"#,
        )
        .bind(event_id)
        .bind(workflow_id)
        .bind(sequence)
        .bind(event_type)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn complete_workflow(&self, workflow_id: &str, event_id: &str) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        let result = sqlx::query(
            r#"UPDATE workflows
               SET state = 'completed',
                   completed_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE workflow_id = ? AND state = 'running'"#,
        )
        .bind(workflow_id)
        .execute(&mut *tx)
        .await?;
        if result.rows_affected() != 1 {
            return Err(Error::StateConflict(format!(
                "workflow {workflow_id} must exist in running state"
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
               VALUES (?, ?, ?, 'workflow.completed', '{}')"#,
        )
        .bind(event_id)
        .bind(workflow_id)
        .bind(sequence)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn idle_unsubmitted_workflow_node(
        &self,
        workflow_id: &str,
        node_id: &str,
        event_id: &str,
    ) -> Result<()> {
        self.transition_running_workflow(RunningWorkflowTransition {
            workflow_id,
            unsubmitted_node_id: Some(node_id),
            event_id,
            state: "idle",
            event_type: "workflow.idle",
            failure_message: None,
            payload: "{}",
        })
        .await
    }

    pub async fn fail_workflow(
        &self,
        workflow_id: &str,
        event_id: &str,
        failure_message: &str,
    ) -> Result<()> {
        let payload = serde_json::json!({ "failure_message": failure_message }).to_string();
        self.transition_running_workflow(RunningWorkflowTransition {
            workflow_id,
            unsubmitted_node_id: None,
            event_id,
            state: "failed",
            event_type: "workflow.failed",
            failure_message: Some(failure_message),
            payload: &payload,
        })
        .await
    }

    pub async fn fail_unsubmitted_workflow_node(
        &self,
        workflow_id: &str,
        node_id: &str,
        event_id: &str,
        failure_message: &str,
    ) -> Result<()> {
        let payload = serde_json::json!({ "failure_message": failure_message }).to_string();
        self.transition_running_workflow(RunningWorkflowTransition {
            workflow_id,
            unsubmitted_node_id: Some(node_id),
            event_id,
            state: "failed",
            event_type: "workflow.failed",
            failure_message: Some(failure_message),
            payload: &payload,
        })
        .await
    }

    async fn transition_running_workflow(
        &self,
        transition: RunningWorkflowTransition<'_>,
    ) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        let result = sqlx::query(
            r#"UPDATE workflows
               SET state = ?,
                   failure_message = ?,
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE workflow_id = ?
                 AND state = 'running'
                 AND (
                     ? IS NULL
                     OR EXISTS (
                         SELECT 1 FROM workflow_nodes
                         WHERE workflow_nodes.workflow_id = workflows.workflow_id
                           AND workflow_nodes.node_id = ?
                           AND workflow_nodes.submitted_at IS NULL
                           AND workflow_nodes.introduced_revision <= workflows.current_revision
                           AND (workflow_nodes.retired_revision IS NULL
                                OR workflow_nodes.retired_revision > workflows.current_revision)
                     )
                 )"#,
        )
        .bind(transition.state)
        .bind(transition.failure_message)
        .bind(transition.workflow_id)
        .bind(transition.unsubmitted_node_id)
        .bind(transition.unsubmitted_node_id)
        .execute(&mut *tx)
        .await?;
        if result.rows_affected() != 1 {
            let expected = transition.unsubmitted_node_id.map_or_else(
                || "running state".to_string(),
                |node_id| format!("running state with unsubmitted node {node_id}"),
            );
            return Err(Error::StateConflict(format!(
                "workflow {} must exist in {expected}",
                transition.workflow_id
            )));
        }
        let sequence: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(sequence), 0) + 1 FROM workflow_events WHERE workflow_id = ?",
        )
        .bind(transition.workflow_id)
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query(
            r#"INSERT INTO workflow_events
               (event_id, workflow_id, sequence, event_type, payload)
               VALUES (?, ?, ?, ?, ?)"#,
        )
        .bind(transition.event_id)
        .bind(transition.workflow_id)
        .bind(sequence)
        .bind(transition.event_type)
        .bind(transition.payload)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }
}
