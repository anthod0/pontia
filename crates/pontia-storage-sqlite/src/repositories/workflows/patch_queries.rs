use pontia_core::Result;

use crate::models::workflows::WorkflowPatchRow;

use super::SqliteWorkflowRepository;

impl SqliteWorkflowRepository {
    pub async fn list_patches(&self, workflow_id: &str) -> Result<Vec<WorkflowPatchRow>> {
        Ok(sqlx::query_as::<_, WorkflowPatchRow>(
            r#"SELECT patch_id, workflow_id, requesting_node_id, requesting_session_id,
                      requesting_turn_id, requesting_runtime_instance_id, replanner_creation_token,
                      replanner_session_id, replanner_turn_id, replanner_runtime_instance_id,
                      base_revision, result_revision, state, request_document_ref,
                      request_size_bytes, decision_document_ref, reason_document_ref,
                      blocked_draft_ref, interruption_attempted_at, interruption_requested_at,
                      replanning_unlocked_at, continuation_message_id, continuation_queued_at,
                      replanner_exit_requested_at, requested_at, planning_at, resolved_at
               FROM workflow_patches
               WHERE workflow_id = ?
               ORDER BY requested_at, patch_id"#,
        )
        .bind(workflow_id)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn get_patch(&self, patch_id: &str) -> Result<Option<WorkflowPatchRow>> {
        Ok(sqlx::query_as::<_, WorkflowPatchRow>(
            r#"SELECT patch_id, workflow_id, requesting_node_id, requesting_session_id,
                      requesting_turn_id, requesting_runtime_instance_id, replanner_creation_token,
                      replanner_session_id, replanner_turn_id, replanner_runtime_instance_id,
                      base_revision, result_revision, state, request_document_ref,
                      request_size_bytes, decision_document_ref, reason_document_ref,
                      blocked_draft_ref, interruption_attempted_at, interruption_requested_at,
                      replanning_unlocked_at, continuation_message_id, continuation_queued_at,
                      replanner_exit_requested_at, requested_at, planning_at, resolved_at
               FROM workflow_patches WHERE patch_id = ?"#,
        )
        .bind(patch_id)
        .fetch_optional(&self.pool)
        .await?)
    }

    pub async fn get_active_patch_for_replanner(
        &self,
        session_id: &str,
        runtime_instance_id: &str,
    ) -> Result<Option<WorkflowPatchRow>> {
        let patch_id: Option<String> = sqlx::query_scalar(
            r#"SELECT p.patch_id FROM workflow_patches AS p
               JOIN workflows AS w ON w.active_patch_id = p.patch_id
               JOIN sessions AS s ON s.session_id = p.replanner_session_id
               JOIN turns AS t ON t.turn_id = s.current_turn_id AND t.session_id = s.session_id
               JOIN runtime_bindings AS r ON r.session_id = s.session_id
               WHERE p.replanner_session_id = ? AND p.replanner_runtime_instance_id = ?
                 AND p.state = 'planning' AND w.state = 'replanning'
                 AND w.active_replanner_session_id = p.replanner_session_id
                 AND r.binding_state = 'confirmed' AND r.runtime_instance_id = ?
                 AND t.state IN ('queued', 'running')"#,
        )
        .bind(session_id)
        .bind(runtime_instance_id)
        .bind(runtime_instance_id)
        .fetch_optional(&self.pool)
        .await?;
        match patch_id {
            Some(patch_id) => self.get_patch(&patch_id).await,
            None => Ok(None),
        }
    }

    pub async fn get_active_patch(&self, workflow_id: &str) -> Result<Option<WorkflowPatchRow>> {
        Ok(sqlx::query_as::<_, WorkflowPatchRow>(
            r#"SELECT p.patch_id, p.workflow_id, p.requesting_node_id, p.requesting_session_id,
                      p.requesting_turn_id, p.requesting_runtime_instance_id,
                      p.replanner_creation_token, p.replanner_session_id, p.replanner_turn_id,
                      p.replanner_runtime_instance_id, p.base_revision, p.result_revision,
                      p.state, p.request_document_ref, p.request_size_bytes,
                      p.decision_document_ref, p.reason_document_ref, p.blocked_draft_ref,
                      p.interruption_attempted_at, p.interruption_requested_at,
                      p.replanning_unlocked_at, p.continuation_message_id,
                      p.continuation_queued_at, p.replanner_exit_requested_at,
                      p.requested_at, p.planning_at, p.resolved_at
               FROM workflows AS w
               JOIN workflow_patches AS p ON p.patch_id = w.active_patch_id
               WHERE w.workflow_id = ? AND w.state = 'replanning'
                 AND p.state IN ('requested', 'planning')"#,
        )
        .bind(workflow_id)
        .fetch_optional(&self.pool)
        .await?)
    }

    pub async fn get_latest_patch(&self, workflow_id: &str) -> Result<Option<WorkflowPatchRow>> {
        let patch_id: Option<String> = sqlx::query_scalar(
            r#"SELECT patch_id FROM workflow_patches
               WHERE workflow_id = ?
               ORDER BY requested_at DESC, patch_id DESC LIMIT 1"#,
        )
        .bind(workflow_id)
        .fetch_optional(&self.pool)
        .await?;
        match patch_id {
            Some(patch_id) => self.get_patch(&patch_id).await,
            None => Ok(None),
        }
    }

    pub async fn get_resolved_patch_for_replanner(
        &self,
        workflow_id: &str,
    ) -> Result<Option<WorkflowPatchRow>> {
        let patch_id: Option<String> = sqlx::query_scalar(
            r#"SELECT patch_id FROM workflow_patches
               WHERE workflow_id = ? AND state IN ('applied', 'rejected', 'blocked')
                 AND replanner_session_id IS NOT NULL
               ORDER BY resolved_at DESC, patch_id DESC LIMIT 1"#,
        )
        .bind(workflow_id)
        .fetch_optional(&self.pool)
        .await?;
        match patch_id {
            Some(patch_id) => self.get_patch(&patch_id).await,
            None => Ok(None),
        }
    }

    pub async fn get_resolved_patch_awaiting_continuation(
        &self,
        workflow_id: &str,
    ) -> Result<Option<WorkflowPatchRow>> {
        let patch_id: Option<String> = sqlx::query_scalar(
            r#"SELECT patch_id FROM workflow_patches
               WHERE workflow_id = ? AND state IN ('applied', 'rejected')
                 AND continuation_message_id IS NOT NULL AND continuation_queued_at IS NULL
               ORDER BY resolved_at, patch_id LIMIT 1"#,
        )
        .bind(workflow_id)
        .fetch_optional(&self.pool)
        .await?;
        match patch_id {
            Some(patch_id) => self.get_patch(&patch_id).await,
            None => Ok(None),
        }
    }
}
