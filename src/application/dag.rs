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
        let updated = sqlx::query(
            r#"UPDATE dag_proposals
               SET state = 'applied', updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE proposal_id = ?"#,
        )
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

    pub async fn apply_patch(&self, task_id: &str, patch: &DagPatch) -> Result<()> {
        ensure_task_exists(&self.pool, task_id).await?;
        self.validate_patch(task_id, patch).await?;

        let mut tx = self.pool.begin().await?;
        append_task_event(
            &mut tx,
            task_id,
            "dag.patch_applied",
            json!({
                "task_id": task_id,
                "summary": patch.summary,
                "operations": patch.operations,
            }),
        )
        .await?;

        let mut temp_id_map = HashMap::new();
        for operation in &patch.operations {
            match operation {
                PatchOperation::AddWorkItem { work_item } => {
                    let work_item_id = new_prefixed_id("wi");
                    if let Some(temp_id) = &work_item.temp_id {
                        temp_id_map.insert(temp_id.clone(), work_item_id.clone());
                    }
                    append_task_event(
                        &mut tx,
                        task_id,
                        "work_item.created",
                        json!({ "work_item": work_item_event_payload(task_id, &work_item_id, work_item) }),
                    )
                    .await?;
                }
                PatchOperation::AddEdge { edge } => {
                    let from = temp_id_map
                        .get(&edge.from_work_item_id)
                        .map(String::as_str)
                        .unwrap_or(&edge.from_work_item_id);
                    let to = temp_id_map
                        .get(&edge.to_work_item_id)
                        .map(String::as_str)
                        .unwrap_or(&edge.to_work_item_id);
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
                PatchOperation::SupersedeWorkItem {
                    work_item_id,
                    reason,
                } => {
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
            }
        }
        tx.commit().await?;

        GraphProjectionService::new(self.pool.clone(), GraphRuntimeConfig::default())
            .project_task(task_id)
            .await?;
        initialize_projection(&self.pool, task_id).await?;
        Ok(())
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

    async fn validate_patch(&self, task_id: &str, patch: &DagPatch) -> Result<()> {
        let mut added_work_items = Vec::new();
        let mut temp_ids = HashSet::new();
        for operation in &patch.operations {
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
                PatchOperation::SupersedeWorkItem { work_item_id, .. } => {
                    ensure_work_item_exists(&self.pool, task_id, work_item_id).await?;
                    ensure_work_item_not_running(&self.pool, task_id, work_item_id).await?;
                }
            }
        }
        self.ensure_profiles_exist(&added_work_items).await?;

        let snapshot = SqliteDagGraphStore::new(self.pool.clone())
            .task_graph(task_id)
            .await?;
        let superseded: HashSet<String> = patch
            .operations
            .iter()
            .filter_map(|operation| match operation {
                PatchOperation::SupersedeWorkItem { work_item_id, .. } => {
                    Some(work_item_id.clone())
                }
                _ => None,
            })
            .collect();
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
        for operation in &patch.operations {
            if let PatchOperation::AddEdge { edge } = operation {
                let from =
                    resolve_patch_ref(&edge.from_work_item_id, &temp_to_generated, &nodes, "from")?;
                let to =
                    resolve_patch_ref(&edge.to_work_item_id, &temp_to_generated, &nodes, "to")?;
                edges.push(WorkItemEdgeDraft {
                    from_work_item_id: from,
                    to_work_item_id: to,
                    edge_type: edge.edge_type.clone(),
                });
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
    let runtime_states: HashMap<String, String> = runtime_rows
        .into_iter()
        .map(|row| (row.get("work_item_id"), row.get("current_state")))
        .collect();
    let active_ids: HashSet<String> = snapshot
        .work_items
        .iter()
        .filter(|work_item| work_item.active)
        .map(|work_item| work_item.work_item_id.clone())
        .collect();

    for work_item in snapshot.work_items.iter().filter(|work_item| {
        work_item.active && !runtime_states.contains_key(&work_item.work_item_id)
    }) {
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
