use pontia_core::Result;

use super::{CreateWorkflowNodeRecord, CreateWorkflowRecord, WorkflowState};

impl WorkflowState {
    pub async fn create_definition(
        &self,
        workflow: CreateWorkflowRecord,
        nodes: Vec<CreateWorkflowNodeRecord>,
    ) -> Result<()> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        sqlx::query(
            r#"INSERT INTO workflows (workflow_id, title, cwd, state, initiator_session_id)
               VALUES (?, ?, ?, ?, ?)"#,
        )
        .bind(workflow.workflow_id)
        .bind(workflow.title)
        .bind(workflow.cwd)
        .bind(workflow.state)
        .bind(workflow.initiator_session_id)
        .execute(&mut *tx)
        .await?;
        for node in nodes {
            sqlx::query(
                r#"INSERT INTO workflow_nodes
                   (node_id, workflow_id, parent_node_id, node_type, phase, title, instructions,
                    inputs, output, execution_profile_id)
                   VALUES (?, ?, ?, 'agent', ?, ?, ?, ?, ?, ?)"#,
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
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn create_workflow(&self, workflow: CreateWorkflowRecord) -> Result<()> {
        sqlx::query(
            r#"INSERT INTO workflows (workflow_id, title, cwd, state, initiator_session_id)
               VALUES (?, ?, ?, ?, ?)"#,
        )
        .bind(workflow.workflow_id)
        .bind(workflow.title)
        .bind(workflow.cwd)
        .bind(workflow.state)
        .bind(workflow.initiator_session_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn create_node(&self, node: CreateWorkflowNodeRecord) -> Result<()> {
        sqlx::query(
            r#"INSERT INTO workflow_nodes
               (node_id, workflow_id, parent_node_id, node_type, phase, title, instructions, inputs,
                output, execution_profile_id)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
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
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
