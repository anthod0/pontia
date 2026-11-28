use pontia_core::{Error, Result};

use crate::models::{WorkflowNodeRow, WorkflowRow};

use super::WorkflowState;

impl WorkflowState {
    pub async fn list_workflows(&self, limit: u32) -> Result<Vec<WorkflowRow>> {
        Ok(sqlx::query_as::<_, WorkflowRow>(
            r#"SELECT workflow_id, title, cwd, state, initiator_session_id, failure_message, created_at,
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
            r#"SELECT workflow_id, title, cwd, state, initiator_session_id, failure_message, created_at,
                      updated_at, started_at, completed_at
               FROM workflows
               WHERE state IN ('running', 'recovering') OR EXISTS (SELECT 1 FROM workflow_events e WHERE e.workflow_id = workflows.workflow_id AND e.sequence > last_report_sequence AND e.event_type IN ('workflow.failed','workflow.idle','workflow.termination_requested','workflow.completed'))
               ORDER BY created_at, workflow_id"#,
        )
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn get_workflow(&self, workflow_id: &str) -> Result<Option<WorkflowRow>> {
        Ok(sqlx::query_as::<_, WorkflowRow>(
            r#"SELECT workflow_id, title, cwd, state, initiator_session_id, failure_message, created_at,
                      updated_at, started_at, completed_at
               FROM workflows WHERE workflow_id = ?"#,
        )
        .bind(workflow_id)
        .fetch_optional(&self.pool)
        .await?)
    }

    pub async fn list_nodes(&self, workflow_id: &str) -> Result<Vec<WorkflowNodeRow>> {
        Ok(sqlx::query_as::<_, WorkflowNodeRow>(
            r#"SELECT node_id, workflow_id, parent_node_id, node_type, phase, title, instructions,
                      inputs, output, execution_profile_id,
                      session_id, submitted_at,
                      submitted_runtime_id, exit_request_started_at, created_at
               FROM workflow_nodes WHERE workflow_id = ? ORDER BY created_at, node_id"#,
        )
        .bind(workflow_id)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn get_node(&self, node_id: &str) -> Result<Option<WorkflowNodeRow>> {
        Ok(sqlx::query_as::<_, WorkflowNodeRow>(
            r#"SELECT node_id, workflow_id, parent_node_id, node_type, phase, title, instructions,
                      inputs, output, execution_profile_id,
                      session_id, submitted_at,
                      submitted_runtime_id, exit_request_started_at, created_at
               FROM workflow_nodes WHERE node_id = ?"#,
        )
        .bind(node_id)
        .fetch_optional(&self.pool)
        .await?)
    }

    pub async fn failure_node_id(&self, workflow_id: &str) -> Result<Option<String>> {
        Ok(sqlx::query_scalar::<_, Option<String>>(
            "SELECT json_extract(payload, '$.node_id') FROM workflow_events WHERE workflow_id = ? AND event_type = 'workflow.failed' ORDER BY sequence DESC LIMIT 1",
        )
        .bind(workflow_id)
        .fetch_optional(&self.pool)
        .await?
        .flatten())
    }

    pub async fn get_node_by_session(&self, session_id: &str) -> Result<Option<WorkflowNodeRow>> {
        let nodes = sqlx::query_as::<_, WorkflowNodeRow>(
            r#"SELECT node_id, workflow_id, parent_node_id, node_type, phase, title, instructions,
                      inputs, output, execution_profile_id,
                      session_id, submitted_at,
                      submitted_runtime_id, exit_request_started_at, created_at
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
