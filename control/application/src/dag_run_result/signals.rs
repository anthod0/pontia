use super::*;

impl DagRunResultService {
    pub async fn raise_tool_signal(
        &self,
        context: &AgentToolContext,
        payload: RaiseSignalPayload,
    ) -> Result<RaiseSignalToolOutcome> {
        validate_signal_kind(&payload.kind)?;
        let (work_item_id, run_id) = match &context.mode {
            AgentToolMode::Execution {
                run_id,
                work_item_id,
            } => (Some(work_item_id.clone()), Some(run_id.clone())),
            AgentToolMode::Planning { .. } => (None, None),
        };
        let signal_id = new_dag_run_result_id("dagsig");
        sqlx::query(
            r#"INSERT INTO dag_signals (
                    signal_id, task_id, work_item_id, run_id, source_session_id,
                    kind, summary, detail, severity, related_refs
               ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(&signal_id)
        .bind(&context.task_id)
        .bind(work_item_id.as_deref())
        .bind(run_id.as_deref())
        .bind(&context.session_id)
        .bind(&payload.kind)
        .bind(&payload.summary)
        .bind(&payload.detail)
        .bind(normalize_severity(&payload.severity))
        .bind(serde_json::to_string(&payload.related_refs)?)
        .execute(&self.pool)
        .await?;
        self.emit_signal_event(SignalEvent {
            task_id: &context.task_id,
            signal_id: &signal_id,
            work_item_id: work_item_id.as_deref(),
            run_id: run_id.as_deref(),
            source_session_id: Some(&context.session_id),
            source: "agent",
            payload: &payload,
        })
        .await?;
        let execution_run = if matches!(&context.mode, AgentToolMode::Execution { .. }) {
            Some(self.run_for_tool_context(context).await?)
        } else {
            None
        };
        if let Some(run) = execution_run.as_ref() {
            self.block_run_for_signal(run, &payload).await?;
        }
        let replanner_started = if payload.kind == "replan_requested" {
            Box::pin(
                DagPlanningService::with_graph(self.pool.clone(), self.graph.clone())
                    .start_replanning_for_signal(&context.task_id, &signal_id),
            )
            .await?;
            true
        } else {
            false
        };
        if let Some(run) = execution_run.as_ref() {
            self.terminate_run_session(run).await?;
            if !replanner_started {
                self.aggregate_task_state(&run.task_id).await?;
            }
        }
        Ok(RaiseSignalToolOutcome {
            signal_id,
            task_id: context.task_id.clone(),
            work_item_id,
            run_id,
            kind: payload.kind,
            state: "open".to_string(),
            replanner_started,
        })
    }

    async fn block_run_for_signal(
        &self,
        run: &RunForTurn,
        payload: &RaiseSignalPayload,
    ) -> Result<()> {
        if is_terminal_run_state(&run.state) {
            return Ok(());
        }
        let next_state = signal_blocking_state(&payload.kind);
        let projection_state = signal_projection_state(&payload.kind);
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            r#"UPDATE work_item_runs
               SET state = ?, output_summary = ?, completed_at = COALESCE(completed_at, strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE run_id = ? AND state IN ('queued', 'running')"#,
        )
        .bind(next_state)
        .bind(&payload.summary)
        .bind(&run.run_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            r#"UPDATE work_item_runtime_projection
               SET current_state = ?, blocked_reason = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE work_item_id = ? AND current_run_id = ?"#,
        )
        .bind(projection_state)
        .bind(&payload.summary)
        .bind(&run.work_item_id)
        .bind(&run.run_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        self.record_task_event(
            &run.task_id,
            "dag.run_blocked_by_signal",
            json!({
                "run_id": run.run_id,
                "work_item_id": run.work_item_id,
                "state": next_state,
                "signal_kind": payload.kind,
                "summary": payload.summary,
            }),
        )
        .await?;
        Ok(())
    }

    pub(super) async fn emit_signal_event(&self, event: SignalEvent<'_>) -> Result<()> {
        self.record_task_event(
            event.task_id,
            "signal.emitted",
            json!({
                "signal_id": event.signal_id,
                "task_id": event.task_id,
                "work_item_id": event.work_item_id,
                "run_id": event.run_id,
                "source_session_id": event.source_session_id,
                "source": event.source,
                "kind": event.payload.kind,
                "summary": event.payload.summary,
                "detail": event.payload.detail,
                "severity": normalize_severity(&event.payload.severity),
                "related_refs": event.payload.related_refs,
                "state": "open",
            }),
        )
        .await?;
        GraphProjectionService::new(self.pool.clone(), self.graph.clone())
            .project_task(event.task_id)
            .await
    }
}
