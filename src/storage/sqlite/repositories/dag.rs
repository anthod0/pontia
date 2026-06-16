use sqlx::SqlitePool;

use crate::{
    error::Result,
    storage::sqlite::models::dag::{
        DagProposalRow, DagSignalRow, WorkItemEdgeRow, WorkItemRow, WorkItemRunRow,
        WorkItemRuntimeProjectionRow,
    },
};

#[derive(Debug, Clone)]
pub struct SqliteDagRepository {
    pool: SqlitePool,
}

impl SqliteDagRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn list_work_items(&self, task_id: &str) -> Result<Vec<WorkItemRow>> {
        Ok(sqlx::query_as::<_, WorkItemRow>(
            r#"SELECT work_item_id, task_id, title, description, kind, action,
                      execution_profile_id, execution_profile_version, active, priority,
                      optional, parallelizable, acceptance_criteria, metadata, created_at,
                      updated_at
               FROM work_items WHERE task_id = ? ORDER BY active DESC, priority DESC, work_item_id"#,
        )
        .bind(task_id)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn list_work_item_edges(&self, task_id: &str) -> Result<Vec<WorkItemEdgeRow>> {
        Ok(sqlx::query_as::<_, WorkItemEdgeRow>(
            r#"SELECT edge_id, task_id, from_work_item_id, to_work_item_id, edge_type, created_at
               FROM work_item_edges WHERE task_id = ? ORDER BY created_at, edge_id"#,
        )
        .bind(task_id)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn list_work_item_runs(&self, task_id: &str) -> Result<Vec<WorkItemRunRow>> {
        Ok(sqlx::query_as::<_, WorkItemRunRow>(
            r#"SELECT run_id, work_item_id, task_id, attempt, state, session_id, turn_id,
                      client_type, execution_profile_id, execution_profile_version,
                      rendered_prompt_ref, output_summary, failure, created_at, updated_at,
                      started_at, completed_at
               FROM work_item_runs WHERE task_id = ? ORDER BY created_at, run_id"#,
        )
        .bind(task_id)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn list_dag_signals(&self, task_id: &str) -> Result<Vec<DagSignalRow>> {
        Ok(sqlx::query_as::<_, DagSignalRow>(
            r#"SELECT signal_id, task_id, work_item_id, run_id, source_session_id, source, kind,
                      summary, detail, severity, related_refs, state, created_at, updated_at
               FROM dag_signals WHERE task_id = ? ORDER BY created_at, signal_id"#,
        )
        .bind(task_id)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn get_dag_signal(&self, signal_id: &str) -> Result<Option<DagSignalRow>> {
        Ok(sqlx::query_as::<_, DagSignalRow>(
            r#"SELECT signal_id, task_id, work_item_id, run_id, source_session_id, source, kind,
                      summary, detail, severity, related_refs, state, created_at, updated_at
               FROM dag_signals WHERE signal_id = ?"#,
        )
        .bind(signal_id)
        .fetch_optional(&self.pool)
        .await?)
    }

    pub async fn list_runtime_projection(
        &self,
        task_id: &str,
    ) -> Result<Vec<WorkItemRuntimeProjectionRow>> {
        Ok(sqlx::query_as::<_, WorkItemRuntimeProjectionRow>(
            r#"SELECT work_item_id, current_run_id, current_state, current_attempt, ready_at,
                      blocked_reason, outcome_state, outcome_reason, replanned_from_state,
                      retry_count, max_retries, priority, optional, parallelizable, session_id,
                      turn_id, updated_at
               FROM work_item_runtime_projection
               WHERE task_id = ?"#,
        )
        .bind(task_id)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn count_open_signals(&self, task_id: &str) -> Result<i64> {
        Ok(sqlx::query_scalar(
            "SELECT COUNT(*) FROM dag_signals WHERE task_id = ? AND state = 'open'",
        )
        .bind(task_id)
        .fetch_one(&self.pool)
        .await?)
    }

    pub async fn count_work_item_runs(&self, task_id: &str) -> Result<i64> {
        Ok(
            sqlx::query_scalar("SELECT COUNT(*) FROM work_item_runs WHERE task_id = ?")
                .bind(task_id)
                .fetch_one(&self.pool)
                .await?,
        )
    }

    pub async fn list_task_dag_proposals(&self, task_id: &str) -> Result<Vec<DagProposalRow>> {
        Ok(sqlx::query_as::<_, DagProposalRow>(
            r#"SELECT proposal_id, task_id, mode, state, summary, proposal_json,
                      validation_json, created_by_session_id, created_by_turn_id, revision,
                      supersedes_proposal_id, created_at, updated_at
               FROM dag_proposals
               WHERE task_id = ?
               ORDER BY revision DESC, created_at DESC, proposal_id DESC"#,
        )
        .bind(task_id)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn list_relevant_dag_proposals(&self, task_id: &str) -> Result<Vec<DagProposalRow>> {
        Ok(sqlx::query_as::<_, DagProposalRow>(
            r#"SELECT proposal_id, task_id, mode, state, summary, proposal_json,
                      validation_json, created_by_session_id, created_by_turn_id, revision,
                      supersedes_proposal_id, created_at, updated_at
               FROM dag_proposals
               WHERE task_id = ? AND state IN ('proposed', 'validated', 'rejected', 'superseded')
               ORDER BY revision DESC, created_at DESC, proposal_id"#,
        )
        .bind(task_id)
        .fetch_all(&self.pool)
        .await?)
    }
}
