use pontia_core::{Error, Result};
use sqlx::SqlitePool;

use crate::models::workflows::{WorkflowEventRow, WorkflowNodeRow, WorkflowRow};

#[derive(Debug, Clone)]
pub struct CreateWorkflowRecord {
    pub workflow_id: String,
    pub title: String,
    pub cwd: String,
    pub state: String,
}

#[derive(Debug, Clone)]
pub struct CreateWorkflowNodeRecord {
    pub node_id: String,
    pub workflow_id: String,
    pub parent_node_id: Option<String>,
    pub phase: String,
    pub title: String,
    pub instructions: String,
    pub inputs: String,
    pub output: String,
    pub execution_profile_id: Option<String>,
    pub execution_profile_version: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SqliteWorkflowRepository {
    pool: SqlitePool,
}

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
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create_definition(
        &self,
        workflow: CreateWorkflowRecord,
        nodes: Vec<CreateWorkflowNodeRecord>,
    ) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            r#"INSERT INTO workflows (workflow_id, title, cwd, state)
               VALUES (?, ?, ?, ?)"#,
        )
        .bind(workflow.workflow_id)
        .bind(workflow.title)
        .bind(workflow.cwd)
        .bind(workflow.state)
        .execute(&mut *tx)
        .await?;
        for node in nodes {
            sqlx::query(
                r#"INSERT INTO workflow_nodes
                   (node_id, workflow_id, parent_node_id, node_type, phase, title, instructions,
                    inputs, output, execution_profile_id, execution_profile_version)
                   VALUES (?, ?, ?, 'agent', ?, ?, ?, ?, ?, ?, ?)"#,
            )
            .bind(node.node_id)
            .bind(node.workflow_id)
            .bind(node.parent_node_id)
            .bind(node.phase)
            .bind(node.title)
            .bind(node.instructions)
            .bind(node.inputs)
            .bind(node.output)
            .bind(node.execution_profile_id)
            .bind(node.execution_profile_version)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn create_workflow(&self, workflow: CreateWorkflowRecord) -> Result<()> {
        sqlx::query(
            r#"INSERT INTO workflows (workflow_id, title, cwd, state)
               VALUES (?, ?, ?, ?)"#,
        )
        .bind(workflow.workflow_id)
        .bind(workflow.title)
        .bind(workflow.cwd)
        .bind(workflow.state)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn create_node(&self, node: CreateWorkflowNodeRecord) -> Result<()> {
        sqlx::query(
            r#"INSERT INTO workflow_nodes
               (node_id, workflow_id, parent_node_id, node_type, phase, title, instructions, inputs,
                output, execution_profile_id, execution_profile_version)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(node.node_id)
        .bind(node.workflow_id)
        .bind(node.parent_node_id)
        .bind("agent")
        .bind(node.phase)
        .bind(node.title)
        .bind(node.instructions)
        .bind(node.inputs)
        .bind(node.output)
        .bind(node.execution_profile_id)
        .bind(node.execution_profile_version)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_workflow(&self, workflow_id: &str) -> Result<Option<WorkflowRow>> {
        Ok(sqlx::query_as::<_, WorkflowRow>(
            r#"SELECT workflow_id, title, cwd, state, failure_message, created_at, updated_at,
                      started_at, completed_at
               FROM workflows WHERE workflow_id = ?"#,
        )
        .bind(workflow_id)
        .fetch_optional(&self.pool)
        .await?)
    }

    pub async fn list_nodes(&self, workflow_id: &str) -> Result<Vec<WorkflowNodeRow>> {
        Ok(sqlx::query_as::<_, WorkflowNodeRow>(
            r#"SELECT node_id, workflow_id, parent_node_id, node_type, phase, title, instructions,
                      inputs, output, execution_profile_id, execution_profile_version, session_id,
                      submitted_at, created_at
               FROM workflow_nodes
               WHERE workflow_id = ?
               ORDER BY created_at, node_id"#,
        )
        .bind(workflow_id)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn get_node(&self, node_id: &str) -> Result<Option<WorkflowNodeRow>> {
        Ok(sqlx::query_as::<_, WorkflowNodeRow>(
            r#"SELECT node_id, workflow_id, parent_node_id, node_type, phase, title, instructions,
                      inputs, output, execution_profile_id, execution_profile_version, session_id,
                      submitted_at, created_at
               FROM workflow_nodes WHERE node_id = ?"#,
        )
        .bind(node_id)
        .fetch_optional(&self.pool)
        .await?)
    }

    pub async fn get_node_by_session(&self, session_id: &str) -> Result<Option<WorkflowNodeRow>> {
        let nodes = sqlx::query_as::<_, WorkflowNodeRow>(
            r#"SELECT node_id, workflow_id, parent_node_id, node_type, phase, title, instructions,
                      inputs, output, execution_profile_id, execution_profile_version, session_id,
                      submitted_at, created_at
               FROM workflow_nodes WHERE session_id = ?
               ORDER BY created_at, node_id
               LIMIT 2"#,
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?;
        match nodes.as_slice() {
            [] => Ok(None),
            [node] => Ok(Some(node.clone())),
            _ => Err(Error::StateConflict(format!(
                "session {session_id} is bound to multiple workflow nodes"
            ))),
        }
    }

    pub async fn bind_node_session(&self, node_id: &str, session_id: &str) -> Result<()> {
        let result = sqlx::query(
            "UPDATE workflow_nodes SET session_id = ? WHERE node_id = ? AND session_id IS NULL",
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

    pub async fn record_node_submission(&self, node_id: &str) -> Result<()> {
        let result = sqlx::query(
            r#"UPDATE workflow_nodes
               SET submitted_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE node_id = ?
                 AND submitted_at IS NULL
                 AND EXISTS (
                     SELECT 1 FROM workflows
                     WHERE workflows.workflow_id = workflow_nodes.workflow_id
                       AND workflows.state = 'running'
                 )"#,
        )
        .bind(node_id)
        .execute(&self.pool)
        .await?;
        if result.rows_affected() != 1 {
            return Err(Error::StateConflict(format!(
                "workflow node {node_id} must be unsubmitted in a running workflow"
            )));
        }
        Ok(())
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
