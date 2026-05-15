use std::collections::{HashMap, HashSet};

use sqlx::{Sqlite, Transaction};
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

        let existing_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM work_items WHERE task_id = ? AND active = 1")
                .bind(task_id)
                .fetch_one(&self.pool)
                .await?;
        if existing_count > 0 {
            return Err(Error::StateConflict(format!(
                "task {task_id} already has an active DAG"
            )));
        }

        let mut tx = self.pool.begin().await?;
        let mut id_map = HashMap::new();
        for draft in &payload.work_items {
            let work_item_id = new_prefixed_id("wi");
            id_map.insert(
                draft.temp_id.clone().unwrap_or_default(),
                work_item_id.clone(),
            );
            insert_work_item(&mut tx, task_id, &work_item_id, draft).await?;
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
            insert_edge(&mut tx, task_id, from, to, &edge.edge_type).await?;
        }
        initialize_projection(&mut tx, task_id).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn apply_patch(&self, task_id: &str, patch: &DagPatch) -> Result<()> {
        ensure_task_exists(&self.pool, task_id).await?;
        self.validate_patch(task_id, patch).await?;

        let mut tx = self.pool.begin().await?;
        let mut temp_id_map = HashMap::new();
        for operation in &patch.operations {
            match operation {
                PatchOperation::AddWorkItem { work_item } => {
                    let work_item_id = new_prefixed_id("wi");
                    if let Some(temp_id) = &work_item.temp_id {
                        temp_id_map.insert(temp_id.clone(), work_item_id.clone());
                    }
                    insert_work_item(&mut tx, task_id, &work_item_id, work_item).await?;
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
                    insert_edge(&mut tx, task_id, from, to, &edge.edge_type).await?;
                }
                PatchOperation::SupersedeWorkItem {
                    work_item_id,
                    reason,
                } => {
                    sqlx::query(
                        r#"UPDATE work_items
                           SET active = 0, metadata = json_set(metadata, '$.superseded_reason', ?),
                               updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                           WHERE task_id = ? AND work_item_id = ?"#,
                    )
                    .bind(reason)
                    .bind(task_id)
                    .bind(work_item_id)
                    .execute(&mut *tx)
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
        initialize_projection(&mut tx, task_id).await?;
        tx.commit().await?;
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

        let mut nodes = existing_work_item_ids(&self.pool, task_id).await?;
        let mut temp_to_generated = HashMap::new();
        for work_item in &added_work_items {
            let generated = format!("__new_{}", temp_to_generated.len());
            if let Some(temp_id) = &work_item.temp_id {
                temp_to_generated.insert(temp_id.clone(), generated.clone());
            }
            nodes.push(generated);
        }

        let mut edges = existing_edges(&self.pool, task_id).await?;
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

async fn insert_work_item(
    tx: &mut Transaction<'_, Sqlite>,
    task_id: &str,
    work_item_id: &str,
    draft: &WorkItemDraft,
) -> Result<()> {
    let acceptance_criteria = serde_json::to_string(&draft.acceptance_criteria)?;
    let metadata = serde_json::to_string(&draft.metadata)?;
    sqlx::query(
        r#"INSERT INTO work_items (
                work_item_id, task_id, title, description, kind, action,
                execution_profile_id, execution_profile_version, active, priority,
                optional, parallelizable, acceptance_criteria, metadata
           ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, 1, ?, ?, ?, ?, ?)"#,
    )
    .bind(work_item_id)
    .bind(task_id)
    .bind(&draft.title)
    .bind(&draft.description)
    .bind(&draft.kind)
    .bind(&draft.action)
    .bind(&draft.execution_profile_id)
    .bind(&draft.execution_profile_version)
    .bind(draft.priority)
    .bind(draft.optional)
    .bind(draft.parallelizable)
    .bind(acceptance_criteria)
    .bind(metadata)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn insert_edge(
    tx: &mut Transaction<'_, Sqlite>,
    task_id: &str,
    from: &str,
    to: &str,
    edge_type: &str,
) -> Result<()> {
    sqlx::query(
        r#"INSERT INTO work_item_edges (edge_id, task_id, from_work_item_id, to_work_item_id, edge_type)
           VALUES (?, ?, ?, ?, ?)"#,
    )
    .bind(new_prefixed_id("wie"))
    .bind(task_id)
    .bind(from)
    .bind(to)
    .bind(edge_type)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn initialize_projection(tx: &mut Transaction<'_, Sqlite>, task_id: &str) -> Result<()> {
    let rows = sqlx::query(
        r#"SELECT wi.work_item_id, wi.priority, wi.optional, wi.parallelizable,
                  EXISTS(
                      SELECT 1 FROM work_item_edges e
                      JOIN work_items upstream ON upstream.work_item_id = e.from_work_item_id
                      LEFT JOIN work_item_runtime_projection up
                        ON up.work_item_id = upstream.work_item_id
                      WHERE e.task_id = wi.task_id
                        AND e.to_work_item_id = wi.work_item_id
                        AND e.edge_type = 'depends_on'
                        AND upstream.active = 1
                        AND COALESCE(up.current_state, 'pending') NOT IN ('completed', 'replan_anchor')
                  ) AS has_blocking_dependency
           FROM work_items wi
           LEFT JOIN work_item_runtime_projection existing
             ON existing.work_item_id = wi.work_item_id
           WHERE wi.task_id = ? AND wi.active = 1 AND existing.work_item_id IS NULL"#,
    )
    .bind(task_id)
    .fetch_all(&mut **tx)
    .await?;

    for row in rows {
        let work_item_id: String = row.get("work_item_id");
        let priority: i64 = row.get("priority");
        let optional: bool = row.get("optional");
        let parallelizable: bool = row.get("parallelizable");
        let has_blocking_dependency: bool = row.get("has_blocking_dependency");
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
        .bind(&work_item_id)
        .bind(task_id)
        .bind(state)
        .bind(ready_at)
        .bind(if state == "blocked" {
            Some("waiting_for_dependencies")
        } else {
            None
        })
        .bind(priority)
        .bind(optional)
        .bind(parallelizable)
        .execute(&mut **tx)
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
    let exists: Option<i64> = sqlx::query_scalar(
        "SELECT 1 FROM work_items WHERE task_id = ? AND work_item_id = ? AND active = 1",
    )
    .bind(task_id)
    .bind(work_item_id)
    .fetch_optional(pool)
    .await?;
    if exists.is_some() {
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

async fn existing_work_item_ids(pool: &SqlitePool, task_id: &str) -> Result<Vec<String>> {
    Ok(
        sqlx::query_scalar("SELECT work_item_id FROM work_items WHERE task_id = ? AND active = 1")
            .bind(task_id)
            .fetch_all(pool)
            .await?,
    )
}

async fn existing_edges(pool: &SqlitePool, task_id: &str) -> Result<Vec<WorkItemEdgeDraft>> {
    let rows = sqlx::query(
        "SELECT from_work_item_id, to_work_item_id, edge_type FROM work_item_edges WHERE task_id = ?",
    )
    .bind(task_id)
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(WorkItemEdgeDraft {
                from_work_item_id: row.get("from_work_item_id"),
                to_work_item_id: row.get("to_work_item_id"),
                edge_type: row.get("edge_type"),
            })
        })
        .collect()
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
