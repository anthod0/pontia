use pontia_core::{Error, Result};

use crate::models::workflows::{WorkflowNodeRow, WorkflowRow};

use super::SqliteWorkflowRepository;

impl SqliteWorkflowRepository {
    pub async fn list_workflows(&self, limit: u32) -> Result<Vec<WorkflowRow>> {
        Ok(sqlx::query_as::<_, WorkflowRow>(
            r#"SELECT workflow_id, title, cwd, state, current_revision, failure_message, created_at,
                      updated_at, started_at, completed_at
               FROM workflows
               ORDER BY created_at DESC, workflow_id DESC
               LIMIT ?"#,
        )
        .bind(i64::from(limit))
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn list_workflows_requiring_convergence(&self) -> Result<Vec<WorkflowRow>> {
        Ok(sqlx::query_as::<_, WorkflowRow>(
            r#"SELECT workflow_id, title, cwd, state, current_revision, failure_message, created_at,
                      updated_at, started_at, completed_at
               FROM workflows
               WHERE state IN ('running', 'paused', 'replanning', 'blocked')
               ORDER BY created_at, workflow_id"#,
        )
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn get_workflow(&self, workflow_id: &str) -> Result<Option<WorkflowRow>> {
        Ok(sqlx::query_as::<_, WorkflowRow>(
            r#"SELECT workflow_id, title, cwd, state, current_revision, failure_message, created_at,
                      updated_at, started_at, completed_at
               FROM workflows WHERE workflow_id = ?"#,
        )
        .bind(workflow_id)
        .fetch_optional(&self.pool)
        .await?)
    }

    pub async fn list_nodes(&self, workflow_id: &str) -> Result<Vec<WorkflowNodeRow>> {
        let Some(workflow) = self.get_workflow(workflow_id).await? else {
            return Ok(Vec::new());
        };
        self.list_nodes_at_revision(workflow_id, workflow.current_revision)
            .await
    }

    pub async fn list_nodes_at_revision(
        &self,
        workflow_id: &str,
        revision: i64,
    ) -> Result<Vec<WorkflowNodeRow>> {
        let Some(workflow) = self.get_workflow(workflow_id).await? else {
            return Ok(Vec::new());
        };
        if revision < 1 || revision > workflow.current_revision {
            return Err(Error::StateConflict(format!(
                "workflow {workflow_id} does not have revision {revision}"
            )));
        }
        Ok(sqlx::query_as::<_, WorkflowNodeRow>(
            r#"SELECT node_id, workflow_id, parent_node_id, node_type, phase, title, instructions,
                      inputs, output, execution_profile_id, execution_profile_version,
                      introduced_revision, retired_revision, session_id, submitted_at,
                      submitted_runtime_instance_id, exit_request_started_at, created_at
               FROM workflow_nodes
               WHERE workflow_id = ?
                 AND introduced_revision <= ?
                 AND (retired_revision IS NULL OR retired_revision > ?)
               ORDER BY created_at, node_id"#,
        )
        .bind(workflow_id)
        .bind(revision)
        .bind(revision)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn list_node_history(&self, workflow_id: &str) -> Result<Vec<WorkflowNodeRow>> {
        Ok(sqlx::query_as::<_, WorkflowNodeRow>(
            r#"SELECT node_id, workflow_id, parent_node_id, node_type, phase, title, instructions,
                      inputs, output, execution_profile_id, execution_profile_version,
                      introduced_revision, retired_revision, session_id, submitted_at,
                      submitted_runtime_instance_id, exit_request_started_at, created_at
               FROM workflow_nodes WHERE workflow_id = ? ORDER BY created_at, node_id"#,
        )
        .bind(workflow_id)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn get_node(&self, node_id: &str) -> Result<Option<WorkflowNodeRow>> {
        Ok(sqlx::query_as::<_, WorkflowNodeRow>(
            r#"SELECT node_id, workflow_id, parent_node_id, node_type, phase, title, instructions,
                      inputs, output, execution_profile_id, execution_profile_version,
                      introduced_revision, retired_revision, session_id, submitted_at,
                      submitted_runtime_instance_id, exit_request_started_at, created_at
               FROM workflow_nodes WHERE node_id = ?"#,
        )
        .bind(node_id)
        .fetch_optional(&self.pool)
        .await?)
    }

    pub async fn get_node_by_session(&self, session_id: &str) -> Result<Option<WorkflowNodeRow>> {
        let nodes = sqlx::query_as::<_, WorkflowNodeRow>(
            r#"SELECT node_id, workflow_id, parent_node_id, node_type, phase, title, instructions,
                      inputs, output, execution_profile_id, execution_profile_version,
                      introduced_revision, retired_revision, session_id, submitted_at,
                      submitted_runtime_instance_id, exit_request_started_at, created_at
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
}
