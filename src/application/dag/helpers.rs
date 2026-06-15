use super::*;

pub(super) async fn append_task_event(
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

pub(super) fn expand_patch_operations(operations: &[PatchOperation]) -> Vec<PatchOperation> {
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

pub(super) fn resolve_temp_id_ref(value: &str, temp_id_map: &HashMap<String, String>) -> String {
    temp_id_map
        .get(value)
        .cloned()
        .unwrap_or_else(|| value.to_string())
}

pub(super) fn work_item_event_payload(
    task_id: &str,
    work_item_id: &str,
    draft: &WorkItemDraft,
) -> Value {
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

pub(super) async fn ensure_task_exists(pool: &SqlitePool, task_id: &str) -> Result<()> {
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

pub(super) fn validate_supersede_policy(policy: &str) -> Result<()> {
    match policy {
        "none" | "explicit_only" | "direct_downstream" | "reachable_downstream" => Ok(()),
        other => Err(Error::Domain(format!(
            "unknown patch supersede_policy {other}"
        ))),
    }
}

pub(super) async fn ensure_work_item_exists(
    pool: &SqlitePool,
    graph: &GraphRuntimeConfig,
    task_id: &str,
    work_item_id: &str,
) -> Result<()> {
    let work_item = GraphProjectionService::new(pool.clone(), graph.clone())
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

pub(super) async fn ensure_work_item_not_running(
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

pub(super) fn resolve_patch_ref(
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

pub(super) fn parse_json_string(raw: String) -> Result<Value> {
    Ok(serde_json::from_str(&raw)?)
}

pub(super) fn new_prefixed_id(prefix: &str) -> String {
    format!("{prefix}_{}", Uuid::now_v7())
}
