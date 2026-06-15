use super::*;

impl DagService {
    pub async fn apply_patch(
        &self,
        task_id: &str,
        patch: &DagPatch,
    ) -> Result<DagPatchApplySummary> {
        ensure_task_exists(&self.pool, task_id).await?;
        self.validate_patch(task_id, patch).await?;

        let expanded_operations = expand_patch_operations(&patch.operations);
        let auto_superseded = self.auto_supersede_work_items(task_id, patch).await?;
        let auto_superseded_for_event = auto_superseded.clone();

        let mut tx = self.pool.begin().await?;
        append_task_event(
            &mut tx,
            task_id,
            "dag.patch_applied",
            json!({
                "task_id": task_id,
                "summary": patch.summary,
                "base_revision": patch.base_revision,
                "anchor_work_item_id": patch.anchor_work_item_id,
                "supersede_policy": patch.supersede_policy,
                "auto_superseded_work_item_ids": auto_superseded_for_event,
                "operations": patch.operations,
                "expanded_operations": expanded_operations,
            }),
        )
        .await?;

        let mut applied_supersedes = HashSet::new();
        let mut superseded_work_item_ids = Vec::new();
        let mut added_work_item_ids = Vec::new();
        for work_item_id in auto_superseded {
            applied_supersedes.insert(work_item_id.clone());
            superseded_work_item_ids.push(work_item_id.clone());
            let reason = format!(
                "replanned_by_anchor:{}",
                patch.anchor_work_item_id.as_deref().unwrap_or_default()
            );
            append_task_event(
                &mut tx,
                task_id,
                "work_item.superseded",
                json!({
                    "task_id": task_id,
                    "work_item_id": work_item_id,
                    "reason": reason,
                }),
            )
            .await?;
            sqlx::query(
                r#"UPDATE work_item_runtime_projection
                   SET current_state = 'superseded', blocked_reason = ?,
                       updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                   WHERE task_id = ? AND work_item_id = ?"#,
            )
            .bind(&reason)
            .bind(task_id)
            .bind(&work_item_id)
            .execute(&mut *tx)
            .await?;
        }

        let mut temp_id_map = HashMap::new();
        for operation in &expanded_operations {
            match operation {
                PatchOperation::AddWorkItem { work_item } => {
                    let work_item_id = new_prefixed_id("wi");
                    if let Some(temp_id) = &work_item.temp_id {
                        temp_id_map.insert(temp_id.clone(), work_item_id.clone());
                    }
                    added_work_item_ids.push(work_item_id.clone());
                    append_task_event(
                        &mut tx,
                        task_id,
                        "work_item.created",
                        json!({ "work_item": work_item_event_payload(task_id, &work_item_id, work_item) }),
                    )
                    .await?;
                }
                PatchOperation::AddEdge { edge } => {
                    let from = resolve_temp_id_ref(&edge.from_work_item_id, &temp_id_map);
                    let to = resolve_temp_id_ref(&edge.to_work_item_id, &temp_id_map);
                    append_task_event(
                        &mut tx,
                        task_id,
                        "work_item.edge_added",
                        json!({
                            "task_id": task_id,
                            "from_work_item_id": from,
                            "to_work_item_id": to,
                            "edge_type": edge.edge_type,
                        }),
                    )
                    .await?;
                }
                PatchOperation::RemoveEdge { edge } => {
                    let from = resolve_temp_id_ref(&edge.from_work_item_id, &temp_id_map);
                    let to = resolve_temp_id_ref(&edge.to_work_item_id, &temp_id_map);
                    append_task_event(
                        &mut tx,
                        task_id,
                        "work_item.edge_removed",
                        json!({
                            "task_id": task_id,
                            "from_work_item_id": from,
                            "to_work_item_id": to,
                            "edge_type": edge.edge_type,
                        }),
                    )
                    .await?;
                }
                PatchOperation::SupersedeWorkItem {
                    work_item_id,
                    reason,
                } => {
                    if !applied_supersedes.insert(work_item_id.clone()) {
                        continue;
                    }
                    superseded_work_item_ids.push(work_item_id.clone());
                    append_task_event(
                        &mut tx,
                        task_id,
                        "work_item.superseded",
                        json!({
                            "task_id": task_id,
                            "work_item_id": work_item_id,
                            "reason": reason,
                        }),
                    )
                    .await?;
                    sqlx::query(
                        r#"UPDATE work_item_runtime_projection
                           SET current_state = 'superseded', blocked_reason = ?,
                               updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                           WHERE task_id = ? AND work_item_id = ?"#,
                    )
                    .bind(reason)
                    .bind(task_id)
                    .bind(work_item_id)
                    .execute(&mut *tx)
                    .await?;
                }
                PatchOperation::ReactivateWorkItem {
                    work_item_id,
                    reason,
                } => {
                    append_task_event(
                        &mut tx,
                        task_id,
                        "work_item.reactivated",
                        json!({
                            "task_id": task_id,
                            "work_item_id": work_item_id,
                            "reason": reason,
                        }),
                    )
                    .await?;
                    sqlx::query(
                        r#"UPDATE work_item_runtime_projection
                           SET current_state = 'blocked', blocked_reason = 'waiting_for_dependencies',
                               updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                           WHERE task_id = ? AND work_item_id = ? AND current_state = 'superseded'"#,
                    )
                    .bind(task_id)
                    .bind(work_item_id)
                    .execute(&mut *tx)
                    .await?;
                }
                PatchOperation::SetWorkItemOutcome {
                    work_item_id,
                    outcome_state,
                    reason,
                } => {
                    append_task_event(
                        &mut tx,
                        task_id,
                        "work_item.outcome_set",
                        json!({
                            "task_id": task_id,
                            "work_item_id": work_item_id,
                            "outcome_state": outcome_state,
                            "reason": reason,
                        }),
                    )
                    .await?;
                    let replan_anchor_state = patch
                        .anchor_work_item_id
                        .as_deref()
                        .filter(|anchor| *anchor == work_item_id)
                        .map(|_| "replan_anchor");
                    sqlx::query(
                        r#"UPDATE work_item_runtime_projection
                           SET outcome_state = ?, outcome_reason = ?,
                               replanned_from_state = COALESCE(replanned_from_state, current_state),
                               current_state = COALESCE(?, current_state),
                               updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                           WHERE task_id = ? AND work_item_id = ?"#,
                    )
                    .bind(outcome_state)
                    .bind(reason)
                    .bind(replan_anchor_state)
                    .bind(task_id)
                    .bind(work_item_id)
                    .execute(&mut *tx)
                    .await?;
                }
                PatchOperation::ReplaceEdge { .. }
                | PatchOperation::InsertWorkItemBetween { .. }
                | PatchOperation::ReplaceDownstream { .. } => {}
            }
        }
        tx.commit().await?;

        GraphProjectionService::new(self.pool.clone(), self.graph.clone())
            .project_task(task_id)
            .await?;
        initialize_projection(&self.pool, &self.graph, task_id).await?;
        Ok(DagPatchApplySummary {
            anchor_work_item_id: patch.anchor_work_item_id.clone(),
            supersede_policy: patch.supersede_policy.clone(),
            superseded_work_item_ids,
            added_work_item_ids,
        })
    }
}
