use super::*;

pub(super) async fn initialize_projection(
    pool: &SqlitePool,
    graph: &GraphRuntimeConfig,
    task_id: &str,
) -> Result<()> {
    let snapshot = GraphProjectionService::new(pool.clone(), graph.clone())
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
