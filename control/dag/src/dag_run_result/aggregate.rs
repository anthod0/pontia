use super::*;

impl DagRunResultService {
    pub(super) async fn aggregate_task_state(&self, task_id: &str) -> Result<()> {
        let rows = sqlx::query(
            r#"SELECT current_state, optional
               FROM work_item_runtime_projection
               WHERE task_id = ? AND current_state != 'superseded'"#,
        )
        .bind(task_id)
        .fetch_all(&self.pool)
        .await?;
        if rows.is_empty() {
            return Ok(());
        }

        let mut required = Vec::new();
        let mut all = Vec::new();
        for row in rows {
            let state: String = row.try_get("current_state")?;
            let optional: bool = row.try_get("optional")?;
            if !optional {
                required.push(state.clone());
            }
            all.push(state);
        }
        let considered = if required.is_empty() { &all } else { &required };

        let next_state = if considered
            .iter()
            .all(|state| matches!(state.as_str(), "completed" | "replan_anchor"))
        {
            "completed"
        } else if considered.iter().any(|state| state == "failed") {
            "failed"
        } else if considered
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
