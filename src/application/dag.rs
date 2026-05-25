use std::collections::{HashMap, HashSet};

use sqlx::Transaction;
use uuid::Uuid;

use super::*;

#[derive(Clone)]
pub struct DagService {
    pool: SqlitePool,
}

impl DagService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn save_proposal(
        &self,
        task_id: &str,
        payload: &SubmitPlanPayload,
        created_by_session_id: Option<&str>,
    ) -> Result<DagProposal> {
        ensure_task_exists(&self.pool, task_id).await?;
        let proposal_id = new_prefixed_id("dagprop");
        let proposal_json = serde_json::to_string(payload)?;
        sqlx::query(
            r#"INSERT INTO dag_proposals (
                    proposal_id, task_id, mode, state, summary, proposal_json,
                    validation_json, created_by_session_id
               ) VALUES (?, ?, ?, 'proposed', ?, ?, '{}', ?)"#,
        )
        .bind(&proposal_id)
        .bind(task_id)
        .bind(&payload.mode)
        .bind(&payload.summary)
        .bind(proposal_json)
        .bind(created_by_session_id)
        .execute(&self.pool)
        .await?;

        self.get_proposal(&proposal_id).await
    }

    pub async fn save_patch_proposal(
        &self,
        task_id: &str,
        summary: &str,
        patch: &DagPatch,
        created_by_session_id: Option<&str>,
    ) -> Result<DagProposal> {
        ensure_task_exists(&self.pool, task_id).await?;
        let proposal_id = new_prefixed_id("dagprop");
        let proposal_json = serde_json::to_string(&json!({
            "mode": "patch",
            "summary": summary,
            "patch": patch,
        }))?;
        sqlx::query(
            r#"INSERT INTO dag_proposals (
                    proposal_id, task_id, mode, state, summary, proposal_json,
                    validation_json, created_by_session_id
               ) VALUES (?, ?, 'patch', 'proposed', ?, ?, '{}', ?)"#,
        )
        .bind(&proposal_id)
        .bind(task_id)
        .bind(summary)
        .bind(proposal_json)
        .bind(created_by_session_id)
        .execute(&self.pool)
        .await?;

        self.get_proposal(&proposal_id).await
    }

    pub async fn mark_proposal_applied(&self, proposal_id: &str) -> Result<DagProposal> {
        self.mark_proposal_applied_with_result(proposal_id, json!({ "ok": true }))
            .await
    }

    pub async fn mark_proposal_applied_with_result(
        &self,
        proposal_id: &str,
        apply_result: Value,
    ) -> Result<DagProposal> {
        let validation_json = serde_json::to_string(&json!({
            "ok": true,
            "apply_result": apply_result,
        }))?;
        let updated = sqlx::query(
            r#"UPDATE dag_proposals
               SET state = 'applied', validation_json = ?,
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE proposal_id = ?"#,
        )
        .bind(validation_json)
        .bind(proposal_id)
        .execute(&self.pool)
        .await?
        .rows_affected();
        if updated == 0 {
            return Err(Error::NotFound(format!("proposal {proposal_id}")));
        }
        self.get_proposal(proposal_id).await
    }

    pub async fn mark_proposal_rejected(
        &self,
        proposal_id: &str,
        message: &str,
    ) -> Result<DagProposal> {
        let validation_json = serde_json::to_string(&json!({
            "ok": false,
            "error": message,
        }))?;
        let updated = sqlx::query(
            r#"UPDATE dag_proposals
               SET state = 'rejected', validation_json = ?,
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE proposal_id = ?"#,
        )
        .bind(validation_json)
        .bind(proposal_id)
        .execute(&self.pool)
        .await?
        .rows_affected();
        if updated == 0 {
            return Err(Error::NotFound(format!("proposal {proposal_id}")));
        }
        self.get_proposal(proposal_id).await
    }

    pub async fn apply_initial_dag(
        &self,
        task_id: &str,
        payload: &SubmitPlanPayload,
    ) -> Result<()> {
        ensure_task_exists(&self.pool, task_id).await?;
        dag_validator::validate_plan_shape(payload)?;
        self.ensure_profiles_exist(&payload.work_items).await?;

        let graph_store = SqliteDagGraphStore::new(self.pool.clone());
        if graph_store
            .task_graph(task_id)
            .await?
            .work_items
            .iter()
            .any(|work_item| work_item.active)
        {
            return Err(Error::StateConflict(format!(
                "task {task_id} already has an active DAG"
            )));
        }

        let mut tx = self.pool.begin().await?;
        append_task_event(
            &mut tx,
            task_id,
            "dag.applied",
            json!({
                "task_id": task_id,
                "summary": payload.summary,
                "assumptions": payload.assumptions,
                "risks": payload.risks,
            }),
        )
        .await?;

        let mut id_map = HashMap::new();
        for draft in &payload.work_items {
            let work_item_id = new_prefixed_id("wi");
            id_map.insert(
                draft.temp_id.clone().unwrap_or_default(),
                work_item_id.clone(),
            );
            append_task_event(
                &mut tx,
                task_id,
                "work_item.created",
                json!({ "work_item": work_item_event_payload(task_id, &work_item_id, draft) }),
            )
            .await?;
        }
        for edge in &payload.edges {
            let from = id_map.get(&edge.from_work_item_id).ok_or_else(|| {
                Error::Domain(format!(
                    "edge references unknown from work item {}",
                    edge.from_work_item_id
                ))
            })?;
            let to = id_map.get(&edge.to_work_item_id).ok_or_else(|| {
                Error::Domain(format!(
                    "edge references unknown to work item {}",
                    edge.to_work_item_id
                ))
            })?;
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
        tx.commit().await?;

        GraphProjectionService::new(self.pool.clone(), GraphRuntimeConfig::default())
            .project_task(task_id)
            .await?;
        initialize_projection(&self.pool, task_id).await?;
        Ok(())
    }

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
                    let from = resolve_runtime_ref(&edge.from_work_item_id, &temp_id_map);
                    let to = resolve_runtime_ref(&edge.to_work_item_id, &temp_id_map);
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
                    let from = resolve_runtime_ref(&edge.from_work_item_id, &temp_id_map);
                    let to = resolve_runtime_ref(&edge.to_work_item_id, &temp_id_map);
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

        GraphProjectionService::new(self.pool.clone(), GraphRuntimeConfig::default())
            .project_task(task_id)
            .await?;
        initialize_projection(&self.pool, task_id).await?;
        Ok(DagPatchApplySummary {
            anchor_work_item_id: patch.anchor_work_item_id.clone(),
            supersede_policy: patch.supersede_policy.clone(),
            superseded_work_item_ids,
            added_work_item_ids,
        })
    }

    async fn get_proposal(&self, proposal_id: &str) -> Result<DagProposal> {
        let row = sqlx::query(
            r#"SELECT proposal_id, task_id, mode, state, summary, proposal_json,
                      validation_json, created_by_session_id, created_at, updated_at
               FROM dag_proposals WHERE proposal_id = ?"#,
        )
        .bind(proposal_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(DagProposal {
            proposal_id: row.get("proposal_id"),
            task_id: row.get("task_id"),
            mode: row.get("mode"),
            state: row.get("state"),
            summary: row.get("summary"),
            proposal_json: parse_json_string(row.get("proposal_json"))?,
            validation_json: parse_json_string(row.get("validation_json"))?,
            created_by_session_id: row.get("created_by_session_id"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        })
    }

    async fn ensure_profiles_exist(&self, work_items: &[WorkItemDraft]) -> Result<()> {
        for work_item in work_items {
            let exists: Option<i64> = if let Some(version) = &work_item.execution_profile_version {
                sqlx::query_scalar(
                    "SELECT 1 FROM execution_profiles WHERE profile_id = ? AND version = ?",
                )
                .bind(&work_item.execution_profile_id)
                .bind(version)
                .fetch_optional(&self.pool)
                .await?
            } else {
                sqlx::query_scalar("SELECT 1 FROM execution_profiles WHERE profile_id = ? LIMIT 1")
                    .bind(&work_item.execution_profile_id)
                    .fetch_optional(&self.pool)
                    .await?
            };
            if exists.is_none() {
                return Err(Error::Domain(format!(
                    "execution profile {}{} does not exist",
                    work_item.execution_profile_id,
                    work_item
                        .execution_profile_version
                        .as_ref()
                        .map(|version| format!(" version {version}"))
                        .unwrap_or_default()
                )));
            }
        }
        Ok(())
    }

    async fn auto_supersede_work_items(
        &self,
        task_id: &str,
        patch: &DagPatch,
    ) -> Result<Vec<String>> {
        match patch.supersede_policy.as_str() {
            "none" | "explicit_only" => return Ok(Vec::new()),
            "direct_downstream" | "reachable_downstream" => {}
            other => {
                return Err(Error::Domain(format!(
                    "unknown patch supersede_policy {other}"
                )));
            }
        }
        let anchor = patch.anchor_work_item_id.as_deref().ok_or_else(|| {
            Error::Domain(format!(
                "patch supersede_policy {} requires anchor_work_item_id",
                patch.supersede_policy
            ))
        })?;

        let snapshot = SqliteDagGraphStore::new(self.pool.clone())
            .task_graph(task_id)
            .await?;
        let active_ids: HashSet<String> = snapshot
            .work_items
            .iter()
            .filter(|work_item| work_item.active)
            .map(|work_item| work_item.work_item_id.clone())
            .collect();
        if !active_ids.contains(anchor) {
            return Err(Error::NotFound(format!("work item {anchor}")));
        }

        let mut candidates = HashSet::new();
        let mut frontier = vec![anchor.to_string()];
        while let Some(from_id) = frontier.pop() {
            for edge in snapshot.edges.iter().filter(|edge| {
                edge.edge_type == GraphEdgeKind::DependsOn
                    && edge.from_work_item_id == from_id
                    && active_ids.contains(&edge.to_work_item_id)
            }) {
                if candidates.insert(edge.to_work_item_id.clone())
                    && patch.supersede_policy == "reachable_downstream"
                {
                    frontier.push(edge.to_work_item_id.clone());
                }
            }
            if patch.supersede_policy == "direct_downstream" {
                break;
            }
        }

        let state_rows = sqlx::query(
            "SELECT work_item_id, current_state FROM work_item_runtime_projection WHERE task_id = ?",
        )
        .bind(task_id)
        .fetch_all(&self.pool)
        .await?;
        let states: HashMap<String, String> = state_rows
            .into_iter()
            .map(|row| (row.get("work_item_id"), row.get("current_state")))
            .collect();

        let mut superseded = Vec::new();
        for work_item_id in candidates {
            match states.get(&work_item_id).map(String::as_str) {
                Some("running") => {
                    return Err(Error::StateConflict(format!(
                        "cannot modify running WorkItem {work_item_id}"
                    )));
                }
                Some("completed") | Some("replan_anchor") | Some("superseded") => {}
                _ => superseded.push(work_item_id),
            }
        }
        Ok(superseded)
    }

    async fn validate_patch(&self, task_id: &str, patch: &DagPatch) -> Result<()> {
        validate_supersede_policy(&patch.supersede_policy)?;
        if patch.supersede_policy != "explicit_only" && patch.supersede_policy != "none" {
            let anchor = patch.anchor_work_item_id.as_deref().ok_or_else(|| {
                Error::Domain(format!(
                    "patch supersede_policy {} requires anchor_work_item_id",
                    patch.supersede_policy
                ))
            })?;
            ensure_work_item_exists(&self.pool, task_id, anchor).await?;
        }

        let expanded_operations = expand_patch_operations(&patch.operations);
        let snapshot = SqliteDagGraphStore::new(self.pool.clone())
            .task_graph(task_id)
            .await?;
        let active_ids: HashSet<String> = snapshot
            .work_items
            .iter()
            .filter(|work_item| work_item.active)
            .map(|work_item| work_item.work_item_id.clone())
            .collect();
        let active_edge_keys: HashSet<(String, String, String)> = snapshot
            .edges
            .iter()
            .map(|edge| {
                (
                    edge.from_work_item_id.clone(),
                    edge.to_work_item_id.clone(),
                    edge.edge_type.as_str().to_string(),
                )
            })
            .collect();

        let mut added_work_items = Vec::new();
        let mut temp_ids = HashSet::new();
        for operation in &expanded_operations {
            match operation {
                PatchOperation::AddWorkItem { work_item } => {
                    dag_validator::validate_work_item_drafts(std::slice::from_ref(work_item))?;
                    if let Some(temp_id) = &work_item.temp_id
                        && !temp_ids.insert(temp_id.clone())
                    {
                        return Err(Error::Domain(format!(
                            "duplicate patch work item temp_id: {temp_id}"
                        )));
                    }
                    added_work_items.push(work_item.clone());
                }
                PatchOperation::AddEdge { edge } => {
                    dag_validator::validate_edge_type(&edge.edge_type)?;
                }
                PatchOperation::RemoveEdge { edge } => {
                    dag_validator::validate_edge_type(&edge.edge_type)?;
                    if !active_edge_keys.contains(&(
                        edge.from_work_item_id.clone(),
                        edge.to_work_item_id.clone(),
                        edge.edge_type.clone(),
                    )) {
                        return Err(Error::NotFound(format!(
                            "active edge {} -> {}",
                            edge.from_work_item_id, edge.to_work_item_id
                        )));
                    }
                }
                PatchOperation::SupersedeWorkItem { work_item_id, .. } => {
                    ensure_work_item_exists(&self.pool, task_id, work_item_id).await?;
                    ensure_work_item_not_running(&self.pool, task_id, work_item_id).await?;
                }
                PatchOperation::ReactivateWorkItem { work_item_id, .. }
                | PatchOperation::SetWorkItemOutcome { work_item_id, .. } => {
                    let exists = SqliteDagGraphStore::new(self.pool.clone())
                        .get_work_item(work_item_id)
                        .await?
                        .is_some_and(|work_item| work_item.task_id == task_id);
                    if !exists {
                        return Err(Error::NotFound(format!("work item {work_item_id}")));
                    }
                    ensure_work_item_not_running(&self.pool, task_id, work_item_id).await?;
                }
                PatchOperation::ReplaceEdge { .. }
                | PatchOperation::InsertWorkItemBetween { .. }
                | PatchOperation::ReplaceDownstream { .. } => {}
            }
        }
        self.ensure_profiles_exist(&added_work_items).await?;

        let mut superseded: HashSet<String> = self
            .auto_supersede_work_items(task_id, patch)
            .await?
            .into_iter()
            .collect();
        superseded.extend(
            expanded_operations
                .iter()
                .filter_map(|operation| match operation {
                    PatchOperation::SupersedeWorkItem { work_item_id, .. } => {
                        Some(work_item_id.clone())
                    }
                    _ => None,
                }),
        );
        let mut nodes: Vec<String> = snapshot
            .work_items
            .iter()
            .filter(|work_item| work_item.active && !superseded.contains(&work_item.work_item_id))
            .map(|work_item| work_item.work_item_id.clone())
            .collect();
        let mut temp_to_generated = HashMap::new();
        for work_item in &added_work_items {
            let generated = format!("__new_{}", temp_to_generated.len());
            if let Some(temp_id) = &work_item.temp_id {
                temp_to_generated.insert(temp_id.clone(), generated.clone());
            }
            nodes.push(generated);
        }

        let mut edges: Vec<WorkItemEdgeDraft> = snapshot
            .edges
            .iter()
            .filter(|edge| {
                edge.edge_type == GraphEdgeKind::DependsOn
                    && !superseded.contains(&edge.from_work_item_id)
                    && !superseded.contains(&edge.to_work_item_id)
            })
            .map(|edge| WorkItemEdgeDraft {
                from_work_item_id: edge.from_work_item_id.clone(),
                to_work_item_id: edge.to_work_item_id.clone(),
                edge_type: edge.edge_type.as_str().to_string(),
            })
            .collect();
        for operation in &expanded_operations {
            match operation {
                PatchOperation::RemoveEdge { edge } => {
                    edges.retain(|existing| {
                        !(existing.from_work_item_id == edge.from_work_item_id
                            && existing.to_work_item_id == edge.to_work_item_id
                            && existing.edge_type == edge.edge_type)
                    });
                }
                PatchOperation::AddEdge { edge } => {
                    let from = resolve_patch_ref(
                        &edge.from_work_item_id,
                        &temp_to_generated,
                        &nodes,
                        "from",
                    )?;
                    let to =
                        resolve_patch_ref(&edge.to_work_item_id, &temp_to_generated, &nodes, "to")?;
                    if active_ids.contains(&from) {
                        ensure_work_item_not_running(&self.pool, task_id, &from).await?;
                    }
                    if active_ids.contains(&to) {
                        ensure_work_item_not_running(&self.pool, task_id, &to).await?;
                    }
                    edges.push(WorkItemEdgeDraft {
                        from_work_item_id: from,
                        to_work_item_id: to,
                        edge_type: edge.edge_type.clone(),
                    });
                }
                _ => {}
            }
        }
        dag_validator::validate_acyclic(nodes, &edges)
    }
}

async fn append_task_event(
    tx: &mut Transaction<'_, sqlx::Sqlite>,
    task_id: &str,
    event_type: &str,
    payload: Value,
) -> Result<()> {
    sqlx::query(
        r#"INSERT INTO task_events (event_id, task_id, event_type, payload)
           VALUES (?, ?, ?, ?)"#,
    )
    .bind(new_event_id().to_string())
    .bind(task_id)
    .bind(event_type)
    .bind(serde_json::to_string(&payload)?)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn expand_patch_operations(operations: &[PatchOperation]) -> Vec<PatchOperation> {
    let mut expanded = Vec::new();
    for operation in operations {
        match operation {
            PatchOperation::ReplaceEdge { from, to } => {
                expanded.push(PatchOperation::RemoveEdge { edge: from.clone() });
                expanded.push(PatchOperation::AddEdge { edge: to.clone() });
            }
            PatchOperation::InsertWorkItemBetween {
                from_work_item_id,
                to_work_item_id,
                work_item,
            } => {
                let temp_id = work_item
                    .temp_id
                    .clone()
                    .unwrap_or_else(|| work_item.title.clone());
                expanded.push(PatchOperation::AddWorkItem {
                    work_item: work_item.clone(),
                });
                expanded.push(PatchOperation::RemoveEdge {
                    edge: WorkItemEdgeDraft {
                        from_work_item_id: from_work_item_id.clone(),
                        to_work_item_id: to_work_item_id.clone(),
                        edge_type: "depends_on".to_string(),
                    },
                });
                expanded.push(PatchOperation::AddEdge {
                    edge: WorkItemEdgeDraft {
                        from_work_item_id: from_work_item_id.clone(),
                        to_work_item_id: temp_id.clone(),
                        edge_type: "depends_on".to_string(),
                    },
                });
                expanded.push(PatchOperation::AddEdge {
                    edge: WorkItemEdgeDraft {
                        from_work_item_id: temp_id,
                        to_work_item_id: to_work_item_id.clone(),
                        edge_type: "depends_on".to_string(),
                    },
                });
            }
            PatchOperation::ReplaceDownstream {
                anchor_work_item_id,
                old_work_item_ids,
                replacement,
                supersede_old,
            } => {
                let temp_id = replacement
                    .temp_id
                    .clone()
                    .unwrap_or_else(|| replacement.title.clone());
                expanded.push(PatchOperation::AddWorkItem {
                    work_item: replacement.clone(),
                });
                for old_id in old_work_item_ids {
                    expanded.push(PatchOperation::RemoveEdge {
                        edge: WorkItemEdgeDraft {
                            from_work_item_id: anchor_work_item_id.clone(),
                            to_work_item_id: old_id.clone(),
                            edge_type: "depends_on".to_string(),
                        },
                    });
                }
                expanded.push(PatchOperation::AddEdge {
                    edge: WorkItemEdgeDraft {
                        from_work_item_id: anchor_work_item_id.clone(),
                        to_work_item_id: temp_id,
                        edge_type: "depends_on".to_string(),
                    },
                });
                if *supersede_old {
                    for old_id in old_work_item_ids {
                        expanded.push(PatchOperation::SupersedeWorkItem {
                            work_item_id: old_id.clone(),
                            reason: "replaced by downstream patch".to_string(),
                        });
                    }
                }
            }
            other => expanded.push(other.clone()),
        }
    }
    expanded
}

fn resolve_runtime_ref(value: &str, temp_id_map: &HashMap<String, String>) -> String {
    temp_id_map
        .get(value)
        .cloned()
        .unwrap_or_else(|| value.to_string())
}

fn work_item_event_payload(task_id: &str, work_item_id: &str, draft: &WorkItemDraft) -> Value {
    json!({
        "work_item_id": work_item_id,
        "task_id": task_id,
        "title": draft.title,
        "description": draft.description,
        "kind": draft.kind,
        "action": draft.action,
        "execution_profile_id": draft.execution_profile_id,
        "execution_profile_version": draft.execution_profile_version,
        "priority": draft.priority,
        "optional": draft.optional,
        "parallelizable": draft.parallelizable,
        "acceptance_criteria": draft.acceptance_criteria,
        "active": true,
        "metadata": draft.metadata,
    })
}

async fn initialize_projection(pool: &SqlitePool, task_id: &str) -> Result<()> {
    let snapshot = SqliteDagGraphStore::new(pool.clone())
        .task_graph(task_id)
        .await?;
    let runtime_rows = sqlx::query(
        "SELECT work_item_id, current_state FROM work_item_runtime_projection WHERE task_id = ?",
    )
    .bind(task_id)
    .fetch_all(pool)
    .await?;
    let mut runtime_states: HashMap<String, String> = runtime_rows
        .into_iter()
        .map(|row| (row.get("work_item_id"), row.get("current_state")))
        .collect();
    let active_ids: HashSet<String> = snapshot
        .work_items
        .iter()
        .filter(|work_item| work_item.active)
        .map(|work_item| work_item.work_item_id.clone())
        .collect();

    let missing_runtime_items = snapshot
        .work_items
        .iter()
        .filter(|work_item| {
            work_item.active && !runtime_states.contains_key(&work_item.work_item_id)
        })
        .cloned()
        .collect::<Vec<_>>();
    for work_item in &missing_runtime_items {
        let has_blocking_dependency = snapshot.edges.iter().any(|edge| {
            edge.edge_type == GraphEdgeKind::DependsOn
                && edge.to_work_item_id == work_item.work_item_id
                && active_ids.contains(&edge.from_work_item_id)
                && !matches!(
                    runtime_states
                        .get(&edge.from_work_item_id)
                        .map(String::as_str),
                    Some("completed") | Some("replan_anchor")
                )
        });
        let state = if has_blocking_dependency {
            "blocked"
        } else {
            "ready"
        };
        let ready_at: Option<&str> = if state == "ready" { Some("now") } else { None };
        sqlx::query(
            r#"INSERT INTO work_item_runtime_projection (
                    work_item_id, task_id, current_state, current_attempt, ready_at,
                    blocked_reason, retry_count, max_retries, priority, optional, parallelizable
               ) VALUES (?, ?, ?, 0,
                    CASE WHEN ? IS NULL THEN NULL ELSE strftime('%Y-%m-%dT%H:%M:%fZ', 'now') END,
                    ?, 0, 0, ?, ?, ?)"#,
        )
        .bind(&work_item.work_item_id)
        .bind(task_id)
        .bind(state)
        .bind(ready_at)
        .bind(if state == "blocked" {
            Some("waiting_for_dependencies")
        } else {
            None
        })
        .bind(work_item.priority)
        .bind(work_item.optional)
        .bind(work_item.parallelizable)
        .execute(pool)
        .await?;
        runtime_states.insert(work_item.work_item_id.clone(), state.to_string());
    }

    for work_item in snapshot
        .work_items
        .iter()
        .filter(|work_item| work_item.active)
    {
        let Some(current_state) = runtime_states
            .get(&work_item.work_item_id)
            .map(String::as_str)
        else {
            continue;
        };
        if !matches!(current_state, "pending" | "ready" | "blocked") {
            continue;
        }
        let has_blocking_dependency = snapshot.edges.iter().any(|edge| {
            edge.edge_type == GraphEdgeKind::DependsOn
                && edge.to_work_item_id == work_item.work_item_id
                && active_ids.contains(&edge.from_work_item_id)
                && !matches!(
                    runtime_states
                        .get(&edge.from_work_item_id)
                        .map(String::as_str),
                    Some("completed") | Some("replan_anchor")
                )
        });
        let next_state = if has_blocking_dependency {
            "blocked"
        } else {
            "ready"
        };
        if next_state != current_state {
            sqlx::query(
                r#"UPDATE work_item_runtime_projection
                   SET current_state = ?,
                       ready_at = CASE WHEN ? = 'ready' THEN COALESCE(ready_at, strftime('%Y-%m-%dT%H:%M:%fZ', 'now')) ELSE NULL END,
                       blocked_reason = CASE WHEN ? = 'blocked' THEN 'waiting_for_dependencies' ELSE NULL END,
                       updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                   WHERE task_id = ? AND work_item_id = ?"#,
            )
            .bind(next_state)
            .bind(next_state)
            .bind(next_state)
            .bind(task_id)
            .bind(&work_item.work_item_id)
            .execute(pool)
            .await?;
            runtime_states.insert(work_item.work_item_id.clone(), next_state.to_string());
        }
    }
    Ok(())
}

async fn ensure_task_exists(pool: &SqlitePool, task_id: &str) -> Result<()> {
    let exists: Option<i64> = sqlx::query_scalar("SELECT 1 FROM tasks WHERE task_id = ?")
        .bind(task_id)
        .fetch_optional(pool)
        .await?;
    if exists.is_some() {
        Ok(())
    } else {
        Err(Error::NotFound(format!("task {task_id}")))
    }
}

fn validate_supersede_policy(policy: &str) -> Result<()> {
    match policy {
        "none" | "explicit_only" | "direct_downstream" | "reachable_downstream" => Ok(()),
        other => Err(Error::Domain(format!(
            "unknown patch supersede_policy {other}"
        ))),
    }
}

async fn ensure_work_item_exists(
    pool: &SqlitePool,
    task_id: &str,
    work_item_id: &str,
) -> Result<()> {
    let work_item = SqliteDagGraphStore::new(pool.clone())
        .get_work_item(work_item_id)
        .await?;
    if work_item
        .as_ref()
        .is_some_and(|work_item| work_item.task_id == task_id && work_item.active)
    {
        Ok(())
    } else {
        Err(Error::NotFound(format!("work item {work_item_id}")))
    }
}

async fn ensure_work_item_not_running(
    pool: &SqlitePool,
    task_id: &str,
    work_item_id: &str,
) -> Result<()> {
    let state: Option<String> = sqlx::query_scalar(
        "SELECT current_state FROM work_item_runtime_projection WHERE task_id = ? AND work_item_id = ?",
    )
    .bind(task_id)
    .bind(work_item_id)
    .fetch_optional(pool)
    .await?;
    if state.as_deref() == Some("running") {
        return Err(Error::StateConflict(format!(
            "cannot modify running WorkItem {work_item_id}"
        )));
    }
    Ok(())
}

fn resolve_patch_ref(
    value: &str,
    temp_to_generated: &HashMap<String, String>,
    nodes: &[String],
    side: &str,
) -> Result<String> {
    if let Some(generated) = temp_to_generated.get(value) {
        return Ok(generated.clone());
    }
    if nodes.iter().any(|node| node == value) {
        return Ok(value.to_string());
    }
    Err(Error::Domain(format!(
        "edge references unknown {side} work item {value}"
    )))
}

fn parse_json_string(raw: String) -> Result<Value> {
    Ok(serde_json::from_str(&raw)?)
}

fn new_prefixed_id(prefix: &str) -> String {
    format!("{prefix}_{}", Uuid::now_v7())
}
