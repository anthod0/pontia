use pontia_core::Result;

use crate::repositories::events::WORKFLOW_TERMINAL_EVENTS;

use super::SqliteWorkflowRepository;

impl SqliteWorkflowRepository {
    pub async fn claim_patch_replanner_exit(&self, patch_id: &str) -> Result<bool> {
        let result = sqlx::query(
            r#"UPDATE workflow_patches SET replanner_exit_requested_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE patch_id = ? AND state IN ('applied', 'rejected', 'blocked')
                 AND replanner_exit_requested_at IS NULL"#,
        )
        .bind(patch_id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn release_patch_replanner_exit(&self, patch_id: &str) -> Result<()> {
        sqlx::query("UPDATE workflow_patches SET replanner_exit_requested_at = NULL WHERE patch_id = ? AND state IN ('applied', 'rejected', 'blocked')")
            .bind(patch_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn mark_patch_continuation_queued(
        &self,
        patch_id: &str,
        message_id: &str,
    ) -> Result<()> {
        sqlx::query(
            r#"UPDATE workflow_patches
               SET continuation_queued_at = COALESCE(
                       continuation_queued_at,
                       strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                   ), updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE patch_id = ? AND state IN ('applied', 'rejected')
                 AND continuation_message_id = ?"#,
        )
        .bind(patch_id)
        .bind(message_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn terminal_event_is_resolved_patch_interruption(
        &self,
        workflow_id: &str,
        event_id: &str,
    ) -> Result<bool> {
        Ok(sqlx::query_scalar::<_, i64>(&format!(
            r#"{WORKFLOW_TERMINAL_EVENTS}
               SELECT COUNT(*) FROM workflow_terminal_events AS e
               JOIN workflow_patches AS p
                 ON p.workflow_id = ? AND p.requesting_session_id = e.session_id
                AND p.requesting_turn_id = e.turn_id
                AND p.state IN ('applied', 'rejected')
                AND e.runtime_instance_id = p.requesting_runtime_instance_id
               WHERE e.event_id = ? AND e.event_type = 'turn.interrupted'"#,
        ))
        .bind(workflow_id)
        .bind(event_id)
        .fetch_one(&self.pool)
        .await?
            > 0)
    }
}
