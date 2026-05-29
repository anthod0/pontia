use super::*;

impl ExternalQueryService {
    pub async fn list_tasks(&self) -> Result<Vec<TaskView>> {
        let rows = sqlx::query(
            r#"SELECT task_id, state, input, workspace_id, session_id, turn_id,
                      routing_state, routing_reason, routing_confidence, metadata,
                      created_at, updated_at
               FROM tasks ORDER BY created_at DESC, task_id"#,
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(row_to_task_view).collect()
    }

    pub async fn get_task(&self, task_id: &str) -> Result<Option<TaskView>> {
        let row = sqlx::query(
            r#"SELECT task_id, state, input, workspace_id, session_id, turn_id,
                      routing_state, routing_reason, routing_confidence, metadata,
                      created_at, updated_at
               FROM tasks WHERE task_id = ?"#,
        )
        .bind(task_id)
        .fetch_optional(&self.pool)
        .await?;

        row.map(row_to_task_view).transpose()
    }

    pub async fn list_task_events(&self, task_id: &str) -> Result<Vec<TaskEventView>> {
        let rows = sqlx::query(
            r#"SELECT event_id, task_id, event_type, payload, created_at
               FROM task_events WHERE task_id = ? ORDER BY created_at, event_id"#,
        )
        .bind(task_id)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(row_to_task_event_view).collect()
    }

    pub async fn list_task_dag_proposals(&self, task_id: &str) -> Result<Vec<DagProposalView>> {
        let rows = sqlx::query(
            r#"SELECT proposal_id, task_id, mode, state, summary, proposal_json,
                      validation_json, created_by_session_id, created_by_turn_id, revision,
                      supersedes_proposal_id, created_at, updated_at
               FROM dag_proposals
               WHERE task_id = ?
               ORDER BY revision DESC, created_at DESC, proposal_id DESC"#,
        )
        .bind(task_id)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(row_to_dag_proposal_view).collect()
    }

    pub async fn list_relevant_dag_proposals(&self, task_id: &str) -> Result<Vec<DagProposal>> {
        let rows = sqlx::query(
            r#"SELECT proposal_id, task_id, mode, state, summary, proposal_json,
                      validation_json, created_by_session_id, created_by_turn_id, revision,
                      supersedes_proposal_id, created_at, updated_at
               FROM dag_proposals
               WHERE task_id = ? AND state IN ('proposed', 'validated', 'rejected', 'superseded')
               ORDER BY revision DESC, created_at DESC, proposal_id"#,
        )
        .bind(task_id)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(row_to_dag_proposal).collect()
    }
}
