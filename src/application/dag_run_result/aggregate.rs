use super::*;

impl DagRunResultService {
    pub(super) async fn aggregate_task_state(&self, task_id: &str) -> Result<()> {
        let graph = SqliteDagGraphStore::new(self.pool.clone())
            .task_graph(task_id)
            .await?;
        let active_items: std::collections::HashMap<String, bool> = graph
            .work_items
            .into_iter()
            .filter(|work_item| work_item.active)
            .map(|work_item| (work_item.work_item_id, work_item.optional))
            .collect();
        if active_items.is_empty() {
            return Ok(());
        }

        let rows = sqlx::query(
            r#"SELECT work_item_id, current_state
               FROM work_item_runtime_projection
               WHERE task_id = ?"#,
        )
        .bind(task_id)
        .fetch_all(&self.pool)
        .await?;

        let mut required = Vec::new();
        for row in rows {
            let work_item_id: String = row.try_get("work_item_id")?;
            let Some(optional) = active_items.get(&work_item_id) else {
                continue;
            };
            if !optional {
                required.push(row.try_get::<String, _>("current_state")?);
            }
        }
        if required.is_empty() {
            return Ok(());
        }

        let next_state = if required
            .iter()
            .all(|state| matches!(state.as_str(), "completed" | "replan_anchor"))
        {
            "completed"
        } else if required.iter().any(|state| state == "failed") {
            "failed"
        } else if required
            .iter()
            .any(|state| matches!(state.as_str(), "blocked" | "needs_input" | "cancelled"))
        {
            "blocked"
        } else {
            "running"
        };

        sqlx::query(
            r#"UPDATE tasks
               SET state = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE task_id = ? AND state NOT IN ('completed', 'failed', 'cancelled', 'replanning', 'paused')"#,
        )
        .bind(next_state)
        .bind(task_id)
        .execute(&self.pool)
        .await?;
        self.record_task_event(
            task_id,
            match next_state {
                "completed" => "task.completed",
                "failed" => "task.failed",
                "blocked" => "task.blocked",
                _ => "task.running",
            },
            json!({ "source": "dag_aggregate" }),
        )
        .await?;
        Ok(())
    }

    pub(super) async fn record_task_event(
        &self,
        task_id: &str,
        event_type: &str,
        payload: Value,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO task_events (event_id, task_id, event_type, payload) VALUES (?, ?, ?, ?)",
        )
        .bind(new_event_id().to_string())
        .bind(task_id)
        .bind(event_type)
        .bind(serde_json::to_string(&payload)?)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
