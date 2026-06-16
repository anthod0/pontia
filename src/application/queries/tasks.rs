use super::*;
use crate::storage::sqlite::repositories::tasks::SqliteTaskRepository;

impl ExternalQueryService {
    pub async fn list_tasks(&self) -> Result<Vec<TaskView>> {
        let repository = SqliteTaskRepository::new(self.pool.clone());
        let rows = repository.list_tasks().await?;

        rows.into_iter().map(task_row_to_view).collect()
    }

    pub async fn get_task(&self, task_id: &str) -> Result<Option<TaskView>> {
        let repository = SqliteTaskRepository::new(self.pool.clone());
        let row = repository.get_task(task_id).await?;

        row.map(task_row_to_view).transpose()
    }

    pub async fn list_task_events(&self, task_id: &str) -> Result<Vec<TaskEventView>> {
        let repository = SqliteTaskRepository::new(self.pool.clone());
        let rows = repository.list_task_events(task_id).await?;

        rows.into_iter().map(task_event_row_to_view).collect()
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
