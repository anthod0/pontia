use super::*;

impl DagRunResultService {
    pub(super) async fn run_for_tool_context(
        &self,
        context: &AgentToolContext,
    ) -> Result<RunForTurn> {
        let AgentToolMode::Execution {
            run_id,
            work_item_id,
        } = &context.mode
        else {
            return Err(Error::StateConflict(
                "submitResult requires a DAG execution turn".to_string(),
            ));
        };
        let row = sqlx::query(
            r#"SELECT run_id, work_item_id, task_id, session_id, state
               FROM work_item_runs
               WHERE run_id = ? AND work_item_id = ? AND task_id = ? AND session_id = ? AND turn_id = ?"#,
        )
        .bind(run_id)
        .bind(work_item_id)
        .bind(&context.task_id)
        .bind(&context.session_id)
        .bind(&context.turn_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| {
            Error::StateConflict(format!(
                "current execution context is not authorized for work item run {run_id}"
            ))
        })?;
        Ok(RunForTurn {
            run_id: row.try_get("run_id")?,
            work_item_id: row.try_get("work_item_id")?,
            task_id: row.try_get("task_id")?,
            session_id: row.try_get("session_id")?,
            state: row.try_get("state")?,
        })
    }

    pub(super) async fn run_for_turn(&self, turn_id: &str) -> Result<Option<RunForTurn>> {
        let row = sqlx::query(
            r#"SELECT run_id, work_item_id, task_id, session_id, state
               FROM work_item_runs WHERE turn_id = ?
               ORDER BY created_at DESC, run_id DESC LIMIT 1"#,
        )
        .bind(turn_id)
        .fetch_optional(&self.pool)
        .await?;

        row.map(|row| {
            Ok(RunForTurn {
                run_id: row.try_get("run_id")?,
                work_item_id: row.try_get("work_item_id")?,
                task_id: row.try_get("task_id")?,
                session_id: row.try_get("session_id")?,
                state: row.try_get("state")?,
            })
        })
        .transpose()
    }

    pub(super) async fn mark_started(&self, run: &RunForTurn) -> Result<()> {
        sqlx::query(
            r#"UPDATE work_item_runs
               SET state = 'running', started_at = COALESCE(started_at, strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE run_id = ? AND state IN ('queued', 'running')"#,
        )
        .bind(&run.run_id)
        .execute(&self.pool)
        .await?;
        sqlx::query(
            r#"UPDATE work_item_runtime_projection
               SET current_state = 'running', updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE current_run_id = ?"#,
        )
        .bind(&run.run_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub(super) async fn terminate_run_session(&self, run: &RunForTurn) -> Result<()> {
        if let Some(session_id) = run.session_id.as_deref() {
            Box::pin(
                RuntimeControlService::new(self.pool.clone()).terminate_session(session_id, None),
            )
            .await?;
        }
        Ok(())
    }
}
