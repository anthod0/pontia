use pontia_core::{Error, Result};

use crate::{models::workflows::WorkflowPatchRow, repositories::events::WORKFLOW_TERMINAL_EVENTS};

use super::{RequestWorkflowPatchRecord, SqliteWorkflowRepository};

impl SqliteWorkflowRepository {
    pub async fn request_patch(
        &self,
        request: RequestWorkflowPatchRecord,
    ) -> Result<WorkflowPatchRow> {
        let mut tx = self.pool.begin().await?;
        crate::repositories::turns::SqliteTurnRepository::serialize_session_turn_writes_in_tx(
            &mut tx,
            &request.session_id,
        )
        .await?;

        let context = sqlx::query_as::<_, (String, String, String, i64)>(
            r#"SELECT n.workflow_id, n.node_id, t.turn_id, w.current_revision
               FROM workflow_nodes AS n
               JOIN workflows AS w ON w.workflow_id = n.workflow_id
               JOIN runtime_bindings AS r ON r.session_id = n.session_id
               JOIN turns AS t ON t.session_id = n.session_id
               WHERE n.session_id = ?
                 AND n.submitted_at IS NULL
                 AND n.introduced_revision <= w.current_revision
                 AND (n.retired_revision IS NULL OR n.retired_revision > w.current_revision)
                 AND w.state = 'running'
                 AND w.active_patch_id IS NULL
                 AND w.activating_node_id IS NULL
                 AND r.binding_state = 'confirmed'
                 AND r.runtime_instance_id = ?
                 AND t.state IN ('queued', 'running')
                 AND NOT EXISTS (
                     SELECT 1 FROM workflow_nodes AS child
                     WHERE child.workflow_id = n.workflow_id
                       AND child.parent_node_id = n.node_id
                       AND child.session_id IS NOT NULL
                       AND child.introduced_revision <= w.current_revision
                       AND (child.retired_revision IS NULL
                            OR child.retired_revision > w.current_revision)
                 )
               ORDER BY t.turn_id
               LIMIT 2"#,
        )
        .bind(&request.session_id)
        .bind(&request.runtime_instance_id)
        .fetch_all(&mut *tx)
        .await?;
        let [(workflow_id, node_id, turn_id, base_revision)] = context.as_slice() else {
            return Err(Error::StateConflict(format!(
                "session {} is not the current unsubmitted Agent Node of a running Workflow with the supplied Runtime and one active Turn",
                request.session_id
            )));
        };

        sqlx::query(
            r#"INSERT INTO workflow_patches
               (patch_id, workflow_id, requesting_node_id, requesting_session_id,
                requesting_turn_id, requesting_runtime_instance_id, replanner_creation_token,
                base_revision, state, request_document_ref, request_size_bytes)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'requested', ?, ?)"#,
        )
        .bind(&request.patch_id)
        .bind(workflow_id)
        .bind(node_id)
        .bind(&request.session_id)
        .bind(turn_id)
        .bind(&request.runtime_instance_id)
        .bind(&request.replanner_creation_token)
        .bind(base_revision)
        .bind(&request.request_document_ref)
        .bind(request.request_size_bytes)
        .execute(&mut *tx)
        .await?;

        let workflow_result = sqlx::query(
            r#"UPDATE workflows
               SET state = 'replanning', active_patch_id = ?,
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE workflow_id = ? AND state = 'running'
                 AND active_patch_id IS NULL AND activating_node_id IS NULL"#,
        )
        .bind(&request.patch_id)
        .bind(workflow_id)
        .execute(&mut *tx)
        .await?;
        if workflow_result.rows_affected() != 1 {
            return Err(Error::StateConflict(format!(
                "workflow {workflow_id} cannot accept a Patch request"
            )));
        }

        let sequence: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(sequence), 0) + 1 FROM workflow_events WHERE workflow_id = ?",
        )
        .bind(workflow_id)
        .fetch_one(&mut *tx)
        .await?;
        let payload = serde_json::json!({
            "patch_id": &request.patch_id,
            "base_revision": base_revision,
            "requesting_node_id": node_id,
            "requesting_session_id": &request.session_id,
            "requesting_turn_id": turn_id,
            "request_document_ref": &request.request_document_ref,
            "request_size_bytes": request.request_size_bytes,
        });
        sqlx::query(
            r#"INSERT INTO workflow_events
               (event_id, workflow_id, sequence, event_type, payload)
               VALUES (?, ?, ?, 'workflow.patch_requested', ?)"#,
        )
        .bind(&request.event_id)
        .bind(workflow_id)
        .bind(sequence)
        .bind(payload.to_string())
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        self.get_patch(&request.patch_id).await?.ok_or_else(|| {
            Error::Domain(format!(
                "accepted Workflow Patch {} is missing",
                request.patch_id
            ))
        })
    }

    pub async fn patch_requester_interrupted(&self, patch_id: &str) -> Result<bool> {
        let result = sqlx::query(&format!(
            r#"{WORKFLOW_TERMINAL_EVENTS}
               UPDATE workflow_patches
               SET replanning_unlocked_at = COALESCE(
                       replanning_unlocked_at,
                       strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                   ),
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE patch_id = ? AND state = 'requested'
                 AND EXISTS (
                     SELECT 1 FROM workflow_terminal_events AS e
                     WHERE e.session_id = workflow_patches.requesting_session_id
                       AND e.turn_id = workflow_patches.requesting_turn_id
                       AND e.event_type = 'turn.interrupted'
                       AND e.runtime_instance_id = workflow_patches.requesting_runtime_instance_id
                 )"#,
        ))
        .bind(patch_id)
        .execute(&self.pool)
        .await?;
        if result.rows_affected() == 1 {
            return Ok(true);
        }
        Ok(sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM workflow_patches WHERE patch_id = ? AND replanning_unlocked_at IS NOT NULL",
        )
        .bind(patch_id)
        .fetch_one(&self.pool)
        .await? == 1)
    }

    pub async fn bind_patch_replanner(
        &self,
        patch_id: &str,
        session_id: &str,
        event_id: &str,
    ) -> Result<bool> {
        let mut tx = self.pool.begin().await?;
        let runtime_instance_id: Option<String> = sqlx::query_scalar(
            r#"SELECT runtime_instance_id FROM runtime_bindings
               WHERE session_id = ? AND binding_state = 'confirmed'
                 AND runtime_instance_id IS NOT NULL"#,
        )
        .bind(session_id)
        .fetch_optional(&mut *tx)
        .await?
        .flatten();
        let Some(runtime_instance_id) = runtime_instance_id else {
            return Ok(false);
        };
        let result = sqlx::query(
            r#"UPDATE workflow_patches
               SET state = 'planning', replanner_session_id = ?,
                   replanner_runtime_instance_id = ?,
                   planning_at = COALESCE(planning_at, strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE patch_id = ? AND state = 'requested'
                 AND replanning_unlocked_at IS NOT NULL
                 AND replanner_session_id IS NULL"#,
        )
        .bind(session_id)
        .bind(&runtime_instance_id)
        .bind(patch_id)
        .execute(&mut *tx)
        .await?;
        if result.rows_affected() == 0 {
            return Ok(false);
        }
        let workflow_id: String =
            sqlx::query_scalar("SELECT workflow_id FROM workflow_patches WHERE patch_id = ?")
                .bind(patch_id)
                .fetch_one(&mut *tx)
                .await?;
        let updated = sqlx::query(
            r#"UPDATE workflows SET active_replanner_session_id = ?,
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE workflow_id = ? AND state = 'replanning' AND active_patch_id = ?
                 AND active_replanner_session_id IS NULL"#,
        )
        .bind(session_id)
        .bind(&workflow_id)
        .bind(patch_id)
        .execute(&mut *tx)
        .await?;
        if updated.rows_affected() != 1 {
            return Err(Error::StateConflict(format!(
                "workflow {workflow_id} cannot bind Re-planner Session {session_id}"
            )));
        }
        let sequence: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(sequence), 0) + 1 FROM workflow_events WHERE workflow_id = ?",
        )
        .bind(&workflow_id)
        .fetch_one(&mut *tx)
        .await?;
        let payload = serde_json::json!({
            "patch_id": patch_id,
            "replanner_session_id": session_id,
            "replanner_runtime_instance_id": runtime_instance_id,
        });
        sqlx::query(
            r#"INSERT INTO workflow_events (event_id, workflow_id, sequence, event_type, payload)
               VALUES (?, ?, ?, 'workflow.replanner_started', ?)"#,
        )
        .bind(event_id)
        .bind(&workflow_id)
        .bind(sequence)
        .bind(payload.to_string())
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(true)
    }

    pub async fn mark_patch_interruption_attempted(&self, patch_id: &str) -> Result<bool> {
        let result = sqlx::query(
            r#"UPDATE workflow_patches
               SET interruption_attempted_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE patch_id = ? AND state = 'requested'
                 AND replanning_unlocked_at IS NULL
                 AND interruption_requested_at IS NULL
                 AND (interruption_attempted_at IS NULL
                      OR interruption_attempted_at < strftime('%Y-%m-%dT%H:%M:%fZ', 'now', '-2 seconds'))"#,
        )
        .bind(patch_id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn mark_patch_interruption_requested(&self, patch_id: &str) -> Result<()> {
        sqlx::query(
            r#"UPDATE workflow_patches
               SET interruption_requested_at = COALESCE(
                       interruption_requested_at,
                       strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                   ),
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE patch_id = ? AND state = 'requested'"#,
        )
        .bind(patch_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
