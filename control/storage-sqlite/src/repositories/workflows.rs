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

impl SqliteWorkflowRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
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
               (node_id, workflow_id, parent_node_id, title, instructions, inputs, output,
                execution_profile_id, execution_profile_version)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(node.node_id)
        .bind(node.workflow_id)
        .bind(node.parent_node_id)
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
            r#"SELECT node_id, workflow_id, parent_node_id, title, instructions, inputs, output,
                      execution_profile_id, execution_profile_version, session_id, submitted_at,
                      created_at
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
            r#"SELECT node_id, workflow_id, parent_node_id, title, instructions, inputs, output,
                      execution_profile_id, execution_profile_version, session_id, submitted_at,
                      created_at
               FROM workflow_nodes WHERE node_id = ?"#,
        )
        .bind(node_id)
        .fetch_optional(&self.pool)
        .await?)
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
               WHERE node_id = ? AND submitted_at IS NULL"#,
        )
        .bind(node_id)
        .execute(&self.pool)
        .await?;
        if result.rows_affected() != 1 {
            return Err(Error::StateConflict(format!(
                "workflow node {node_id} is missing or already submitted"
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
}
