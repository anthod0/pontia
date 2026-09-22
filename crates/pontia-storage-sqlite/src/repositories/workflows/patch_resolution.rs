use pontia_core::{Error, Result};

use crate::models::workflows::WorkflowPatchRow;

use super::{
    ApplyWorkflowPatchRecord, BlockWorkflowPatchRecord, ImplicitBlockWorkflowPatchRecord,
    SqliteWorkflowRepository,
};

impl SqliteWorkflowRepository {
    pub async fn apply_patch(&self, request: ApplyWorkflowPatchRecord) -> Result<WorkflowPatchRow> {
        let mut tx = self.pool.begin().await?;
        crate::repositories::turns::SqliteTurnRepository::serialize_session_turn_writes_in_tx(
            &mut tx,
            &request.session_id,
        )
        .await?;
        let context = sqlx::query_as::<_, (String, String, String, i64, i64)>(
            r#"SELECT p.patch_id, p.workflow_id, t.turn_id, p.base_revision, w.current_revision
               FROM workflow_patches AS p
               JOIN workflows AS w ON w.active_patch_id = p.patch_id
               JOIN sessions AS s ON s.session_id = p.replanner_session_id
               JOIN turns AS t ON t.turn_id = s.current_turn_id AND t.session_id = s.session_id
               JOIN runtime_bindings AS r ON r.session_id = s.session_id
               WHERE p.state = 'planning' AND w.state = 'replanning'
                 AND w.active_replanner_session_id = ?
                 AND p.replanner_session_id = ? AND p.replanner_runtime_instance_id = ?
                 AND r.binding_state = 'confirmed' AND r.runtime_instance_id = ?
                 AND t.state IN ('queued', 'running')"#,
        )
        .bind(&request.session_id)
        .bind(&request.session_id)
        .bind(&request.runtime_instance_id)
        .bind(&request.runtime_instance_id)
        .fetch_all(&mut *tx)
        .await?;
        let [(patch_id, workflow_id, turn_id, base_revision, current_revision)] =
            context.as_slice()
        else {
            return Err(Error::StateConflict(format!(
                "session {} is not the active Re-planner with one active Turn and the supplied Runtime",
                request.session_id
            )));
        };
        if base_revision != current_revision {
            return Err(Error::StateConflict(format!(
                "Workflow Patch {patch_id} base revision {base_revision} does not match current revision {current_revision}"
            )));
        }

        let changed = !request.retired_node_ids.is_empty() || !request.introduced_nodes.is_empty();
        let result_revision = if changed {
            current_revision + 1
        } else {
            *current_revision
        };
        if changed {
            for node_id in &request.retired_node_ids {
                let result = sqlx::query(
                    r#"UPDATE workflow_nodes SET retired_revision = ?
                       WHERE node_id = ? AND workflow_id = ? AND retired_revision IS NULL
                         AND session_id IS NULL AND introduced_revision <= ?"#,
                )
                .bind(result_revision)
                .bind(node_id)
                .bind(workflow_id)
                .bind(current_revision)
                .execute(&mut *tx)
                .await?;
                if result.rows_affected() != 1 {
                    return Err(Error::StateConflict(format!(
                        "Workflow Node {node_id} can no longer be retired by Patch {patch_id}"
                    )));
                }
            }
            for node in &request.introduced_nodes {
                sqlx::query(
                    r#"INSERT INTO workflow_nodes
                       (node_id, workflow_id, parent_node_id, node_type, phase, title, instructions,
                        inputs, output, execution_profile_id, execution_profile_version,
                        introduced_revision)
                       VALUES (?, ?, ?, 'agent', ?, ?, ?, ?, ?, ?, ?, ?)"#,
                )
                .bind(&node.node_id)
                .bind(workflow_id)
                .bind(&node.parent_node_id)
                .bind(&node.phase)
                .bind(&node.title)
                .bind(&node.instructions)
                .bind(&node.inputs)
                .bind(&node.output)
                .bind(&node.execution_profile_id)
                .bind(&node.execution_profile_version)
                .bind(result_revision)
                .execute(&mut *tx)
                .await?;
            }
        }

        let outcome = if changed { "applied" } else { "rejected" };
        let patch_result = sqlx::query(
            r#"UPDATE workflow_patches
               SET state = ?, result_revision = ?, replanner_turn_id = ?,
                   decision_document_ref = ?, continuation_message_id = ?,
                   resolved_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE patch_id = ? AND state = 'planning' AND base_revision = ?
                 AND replanner_session_id = ? AND replanner_runtime_instance_id = ?"#,
        )
        .bind(outcome)
        .bind(result_revision)
        .bind(turn_id)
        .bind(&request.decision_document_ref)
        .bind(&request.continuation_message_id)
        .bind(patch_id)
        .bind(current_revision)
        .bind(&request.session_id)
        .bind(&request.runtime_instance_id)
        .execute(&mut *tx)
        .await?;
        if patch_result.rows_affected() != 1 {
            return Err(Error::StateConflict(format!(
                "Workflow Patch {patch_id} is already resolved"
            )));
        }
        let workflow_result = if changed {
            sqlx::query(
                r#"UPDATE workflows
                   SET state = 'running', current_revision = ?, active_patch_id = NULL,
                       active_replanner_session_id = NULL,
                       updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                   WHERE workflow_id = ? AND state = 'replanning' AND current_revision = ?
                     AND active_patch_id = ? AND active_replanner_session_id = ?"#,
            )
            .bind(result_revision)
            .bind(workflow_id)
            .bind(current_revision)
            .bind(patch_id)
            .bind(&request.session_id)
            .execute(&mut *tx)
            .await?
        } else {
            sqlx::query(
                r#"UPDATE workflows
                   SET state = 'running', active_patch_id = NULL,
                       active_replanner_session_id = NULL,
                       updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                   WHERE workflow_id = ? AND state = 'replanning' AND current_revision = ?
                     AND active_patch_id = ? AND active_replanner_session_id = ?"#,
            )
            .bind(workflow_id)
            .bind(current_revision)
            .bind(patch_id)
            .bind(&request.session_id)
            .execute(&mut *tx)
            .await?
        };
        if workflow_result.rows_affected() != 1 {
            return Err(Error::StateConflict(format!(
                "workflow {workflow_id} can no longer resolve Patch {patch_id}"
            )));
        }

        let sequence: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(sequence), 0) + 1 FROM workflow_events WHERE workflow_id = ?",
        )
        .bind(workflow_id)
        .fetch_one(&mut *tx)
        .await?;
        let added_node_ids = request
            .introduced_nodes
            .iter()
            .map(|node| node.node_id.as_str())
            .collect::<Vec<_>>();
        let payload = serde_json::json!({
            "patch_id": patch_id,
            "base_revision": base_revision,
            "result_revision": result_revision,
            "outcome": outcome,
            "decision_document_ref": &request.decision_document_ref,
            "decision_size_bytes": request.decision_size_bytes,
            "summary": &request.decision_summary,
            "added_node_ids": added_node_ids,
            "retired_node_ids": &request.retired_node_ids,
            "replanner_session_id": &request.session_id,
            "replanner_turn_id": turn_id,
        });
        sqlx::query(
            r#"INSERT INTO workflow_events (event_id, workflow_id, sequence, event_type, payload)
               VALUES (?, ?, ?, ?, ?)"#,
        )
        .bind(&request.event_id)
        .bind(workflow_id)
        .bind(sequence)
        .bind(if changed {
            "workflow.patch_applied"
        } else {
            "workflow.patch_rejected"
        })
        .bind(payload.to_string())
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        self.get_patch(patch_id)
            .await?
            .ok_or_else(|| Error::Domain(format!("resolved Workflow Patch {patch_id} is missing")))
    }

    pub async fn block_patch(&self, request: BlockWorkflowPatchRecord) -> Result<WorkflowPatchRow> {
        let mut tx = self.pool.begin().await?;
        crate::repositories::turns::SqliteTurnRepository::serialize_session_turn_writes_in_tx(
            &mut tx,
            &request.session_id,
        )
        .await?;
        let context = sqlx::query_as::<_, (String, String, String, String)>(
            r#"SELECT p.patch_id, p.workflow_id, t.turn_id, r.runtime_instance_id
               FROM workflow_patches AS p
               JOIN workflows AS w ON w.active_patch_id = p.patch_id
               JOIN sessions AS s ON s.session_id = p.replanner_session_id
               JOIN turns AS t ON t.turn_id = s.current_turn_id AND t.session_id = s.session_id
               JOIN runtime_bindings AS r ON r.session_id = s.session_id
               WHERE p.state = 'planning' AND w.state = 'replanning'
                 AND w.active_replanner_session_id = ?
                 AND p.replanner_session_id = ?
                 AND p.replanner_runtime_instance_id = ?
                 AND r.binding_state = 'confirmed' AND r.runtime_instance_id = ?
                 AND t.state IN ('queued', 'running')"#,
        )
        .bind(&request.session_id)
        .bind(&request.session_id)
        .bind(&request.runtime_instance_id)
        .bind(&request.runtime_instance_id)
        .fetch_all(&mut *tx)
        .await?;
        let [(patch_id, workflow_id, turn_id, _)] = context.as_slice() else {
            return Err(Error::StateConflict(format!(
                "session {} is not the active Re-planner with one active Turn and the supplied Runtime",
                request.session_id
            )));
        };
        let patch_result = sqlx::query(
            r#"UPDATE workflow_patches
               SET state = 'blocked', replanner_turn_id = ?, reason_document_ref = ?,
                   blocked_draft_ref = ?, resolved_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE patch_id = ? AND state = 'planning' AND replanner_session_id = ?
                 AND replanner_runtime_instance_id = ?"#,
        )
        .bind(turn_id)
        .bind(&request.reason_document_ref)
        .bind(&request.blocked_draft_ref)
        .bind(patch_id)
        .bind(&request.session_id)
        .bind(&request.runtime_instance_id)
        .execute(&mut *tx)
        .await?;
        if patch_result.rows_affected() != 1 {
            return Err(Error::StateConflict(format!(
                "Workflow Patch {patch_id} is already resolved"
            )));
        }
        let workflow_result = sqlx::query(
            r#"UPDATE workflows
               SET state = 'blocked', active_patch_id = NULL,
                   active_replanner_session_id = NULL,
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE workflow_id = ? AND state = 'replanning' AND active_patch_id = ?
                 AND active_replanner_session_id = ?"#,
        )
        .bind(workflow_id)
        .bind(patch_id)
        .bind(&request.session_id)
        .execute(&mut *tx)
        .await?;
        if workflow_result.rows_affected() != 1 {
            return Err(Error::StateConflict(format!(
                "workflow {workflow_id} cannot be blocked"
            )));
        }
        let sequence: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(sequence), 0) + 1 FROM workflow_events WHERE workflow_id = ?",
        )
        .bind(workflow_id)
        .fetch_one(&mut *tx)
        .await?;
        let payload = serde_json::json!({
            "patch_id": patch_id,
            "base_revision": sqlx::query_scalar::<_, i64>("SELECT base_revision FROM workflow_patches WHERE patch_id = ?").bind(patch_id).fetch_one(&mut *tx).await?,
            "outcome": "blocked",
            "reason_document_ref": &request.reason_document_ref,
            "blocked_draft_ref": &request.blocked_draft_ref,
            "replanner_session_id": &request.session_id,
            "replanner_turn_id": turn_id,
        });
        sqlx::query(
            r#"INSERT INTO workflow_events (event_id, workflow_id, sequence, event_type, payload)
               VALUES (?, ?, ?, 'workflow.patch_blocked', ?)"#,
        )
        .bind(&request.event_id)
        .bind(workflow_id)
        .bind(sequence)
        .bind(payload.to_string())
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        self.get_patch(patch_id)
            .await?
            .ok_or_else(|| Error::Domain(format!("blocked Workflow Patch {patch_id} is missing")))
    }

    pub async fn implicitly_block_patch(
        &self,
        request: ImplicitBlockWorkflowPatchRecord,
    ) -> Result<bool> {
        let mut tx = self.pool.begin().await?;
        let context = sqlx::query_as::<_, (String, i64, String, Option<String>)>(
            r#"SELECT p.workflow_id, p.base_revision, p.state, p.replanner_session_id
               FROM workflow_patches AS p
               JOIN workflows AS w ON w.active_patch_id = p.patch_id
               WHERE p.patch_id = ? AND p.state IN ('requested', 'planning')
                 AND w.state = 'replanning' AND w.current_revision = p.base_revision
                 AND (p.state = 'requested' OR w.active_replanner_session_id = p.replanner_session_id)"#,
        )
        .bind(&request.patch_id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some((workflow_id, base_revision, patch_state, replanner_session_id)) = context else {
            return Ok(false);
        };

        let patch_result = sqlx::query(
            r#"UPDATE workflow_patches
               SET state = 'blocked', replanner_turn_id = COALESCE(replanner_turn_id, ?),
                   reason_document_ref = ?, blocked_draft_ref = ?,
                   resolved_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE patch_id = ? AND state = ?"#,
        )
        .bind(&request.replanner_turn_id)
        .bind(&request.reason_document_ref)
        .bind(&request.blocked_draft_ref)
        .bind(&request.patch_id)
        .bind(&patch_state)
        .execute(&mut *tx)
        .await?;
        if patch_result.rows_affected() != 1 {
            return Ok(false);
        }
        let workflow_result = sqlx::query(
            r#"UPDATE workflows
               SET state = 'blocked', active_patch_id = NULL,
                   active_replanner_session_id = NULL,
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE workflow_id = ? AND state = 'replanning' AND current_revision = ?
                 AND active_patch_id = ?"#,
        )
        .bind(&workflow_id)
        .bind(base_revision)
        .bind(&request.patch_id)
        .execute(&mut *tx)
        .await?;
        if workflow_result.rows_affected() != 1 {
            return Ok(false);
        }

        let sequence: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(sequence), 0) + 1 FROM workflow_events WHERE workflow_id = ?",
        )
        .bind(&workflow_id)
        .fetch_one(&mut *tx)
        .await?;
        let payload = serde_json::json!({
            "patch_id": &request.patch_id,
            "base_revision": base_revision,
            "outcome": "blocked",
            "implicit": true,
            "reason_document_ref": &request.reason_document_ref,
            "blocked_draft_ref": &request.blocked_draft_ref,
            "summary": &request.reason_summary,
            "replanner_session_id": replanner_session_id,
            "replanner_turn_id": &request.replanner_turn_id,
        });
        sqlx::query(
            r#"INSERT INTO workflow_events (event_id, workflow_id, sequence, event_type, payload)
               VALUES (?, ?, ?, 'workflow.patch_blocked', ?)"#,
        )
        .bind(&request.event_id)
        .bind(&workflow_id)
        .bind(sequence)
        .bind(payload.to_string())
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(true)
    }
}
