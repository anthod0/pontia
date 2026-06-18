use super::*;

impl DagRunResultService {
    pub async fn submit_tool_result(
        &self,
        context: &AgentToolContext,
        payload: SubmitResultPayload,
    ) -> Result<SubmitResultToolOutcome> {
        validate_result_status(&payload.status)?;
        let run = self.run_for_tool_context(context).await?;
        let result = parsed_payload_to_result(payload);
        let state = result.state.clone();
        let scheduler = self
            .handle_terminal_result(
                &TerminalEventRefs {
                    turn_id: Some(context.turn_id.clone()),
                    domain_event_id: None,
                },
                &run,
                result,
            )
            .await?;
        Ok(SubmitResultToolOutcome {
            task_id: run.task_id,
            work_item_id: run.work_item_id,
            run_id: run.run_id,
            state,
            scheduler,
        })
    }

    pub async fn sync_from_turn_event(&self, event: &DomainEvent) -> Result<()> {
        let Some(turn_id) = event.turn_id.as_deref() else {
            return Ok(());
        };
        let Some(run) = self.run_for_turn(turn_id).await? else {
            return Ok(());
        };

        match event.event_type {
            EventType::TurnStarted => {
                self.mark_started(&run).await?;
                Ok(())
            }
            EventType::TurnCompleted => {
                self.handle_terminal(event, &run, self.completed_result(event)?)
                    .await
            }
            EventType::TurnFailed => {
                let summary = failure_summary(&event.payload);
                self.handle_terminal(
                    event,
                    &run,
                    ParsedRunResult {
                        state: "failed".to_string(),
                        summary: summary.clone(),
                        outputs: Vec::new(),
                        failure: Some(json!({ "message": summary })),
                        signals: Vec::new(),
                    },
                )
                .await
            }
            EventType::TurnCancelled | EventType::TurnInterrupted => {
                self.handle_terminal(
                    event,
                    &run,
                    ParsedRunResult {
                        state: "cancelled".to_string(),
                        summary: terminal_summary(&event.payload)
                            .unwrap_or_else(|| event.event_type.to_string()),
                        outputs: Vec::new(),
                        failure: None,
                        signals: Vec::new(),
                    },
                )
                .await
            }
            _ => Ok(()),
        }
    }

    async fn handle_terminal(
        &self,
        event: &DomainEvent,
        run: &RunForTurn,
        result: ParsedRunResult,
    ) -> Result<()> {
        if is_terminal_run_state(&run.state) {
            return Ok(());
        }
        self.handle_terminal_result(
            &TerminalEventRefs {
                turn_id: event.turn_id.clone(),
                domain_event_id: Some(event.event_id.clone()),
            },
            run,
            result,
        )
        .await?;
        Ok(())
    }

    async fn handle_terminal_result(
        &self,
        refs: &TerminalEventRefs,
        run: &RunForTurn,
        result: ParsedRunResult,
    ) -> Result<DagSchedulerOutcome> {
        if is_terminal_run_state(&run.state) {
            return Err(Error::StateConflict(format!(
                "work item run {} is already terminal ({})",
                run.run_id, run.state
            )));
        }

        let failure_json = result
            .failure
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;
        let blocked_reason = if matches!(
            result.state.as_str(),
            "blocked" | "needs_input" | "cancelled"
        ) {
            Some(result.summary.as_str())
        } else {
            None
        };

        let mut signal_ids = Vec::new();
        let mut emitted_signals = Vec::new();
        let mut replan_signal_ids = Vec::new();
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            r#"UPDATE work_item_runs
               SET state = ?, output_summary = ?, failure = ?,
                   completed_at = COALESCE(completed_at, strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE run_id = ?"#,
        )
        .bind(&result.state)
        .bind(&result.summary)
        .bind(failure_json)
        .bind(&run.run_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            r#"UPDATE work_item_runtime_projection
               SET current_state = ?, blocked_reason = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE work_item_id = ? AND current_run_id = ?"#,
        )
        .bind(&result.state)
        .bind(blocked_reason)
        .bind(&run.work_item_id)
        .bind(&run.run_id)
        .execute(&mut *tx)
        .await?;

        for signal in &result.signals {
            let signal_id = new_dag_run_result_id("dagsig");
            if signal.kind == "replan_requested" {
                replan_signal_ids.push(signal_id.clone());
            }
            emitted_signals.push((signal_id.clone(), signal.clone()));
            signal_ids.push(signal_id.clone());
            sqlx::query(
                r#"INSERT INTO dag_signals (
                        signal_id, task_id, work_item_id, run_id, source_session_id,
                        kind, summary, detail, severity, related_refs
                   ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
            )
            .bind(&signal_id)
            .bind(&run.task_id)
            .bind(&run.work_item_id)
            .bind(&run.run_id)
            .bind(run.session_id.as_deref())
            .bind(&signal.kind)
            .bind(&signal.summary)
            .bind(&signal.detail)
            .bind(normalize_severity(&signal.severity))
            .bind(serde_json::to_string(&signal.related_refs)?)
            .execute(&mut *tx)
            .await?;
        }
        if !replan_signal_ids.is_empty() {
            sqlx::query(
                r#"UPDATE work_item_runtime_projection
                   SET current_state = 'replan_anchor', blocked_reason = ?,
                       outcome_state = ?, outcome_reason = ?,
                       replanned_from_state = COALESCE(replanned_from_state, current_state),
                       updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                   WHERE work_item_id = ? AND current_run_id = ?"#,
            )
            .bind(&result.summary)
            .bind(outcome_state_for_status(&result.state))
            .bind(&result.summary)
            .bind(&run.work_item_id)
            .bind(&run.run_id)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;

        for (signal_id, signal) in &emitted_signals {
            self.emit_signal_event(SignalEvent {
                task_id: &run.task_id,
                signal_id,
                work_item_id: Some(&run.work_item_id),
                run_id: Some(&run.run_id),
                source_session_id: run.session_id.as_deref(),
                source: "agent",
                payload: signal,
            })
            .await?;
        }

        self.record_task_event(
            &run.task_id,
            "dag.run_completed",
            json!({
                "run_id": run.run_id,
                "work_item_id": run.work_item_id,
                "turn_id": refs.turn_id,
                "state": result.state,
                "outputs": result.outputs,
                "signals": signal_ids,
                "domain_event_id": refs.domain_event_id,
            }),
        )
        .await?;

        if !replan_signal_ids.is_empty() {
            for signal_id in replan_signal_ids {
                Box::pin(
                    DagPlanningService::with_graph(self.pool.clone(), self.graph.clone())
                        .start_replanning_for_signal(&run.task_id, &signal_id),
                )
                .await?;
            }
            self.terminate_run_session(run).await?;
            return Ok(DagSchedulerOutcome {
                dispatched_runs: Vec::new(),
            });
        }

        let scheduler = if result.state == "completed" {
            Box::pin(
                DagSchedulerService::with_graph(self.pool.clone(), self.graph.clone())
                    .schedule_task(&run.task_id),
            )
            .await?
        } else {
            DagSchedulerOutcome {
                dispatched_runs: Vec::new(),
            }
        };

        self.terminate_run_session(run).await?;
        self.aggregate_task_state(&run.task_id).await?;
        Ok(scheduler)
    }

    fn completed_result(&self, event: &DomainEvent) -> Result<ParsedRunResult> {
        if let Ok(payload) = serde_json::from_value::<SubmitResultPayload>(event.payload.clone()) {
            return Ok(parsed_payload_to_result(payload));
        }
        if let Some(output) = event.payload.get("output")
            && let Ok(payload) = serde_json::from_value::<SubmitResultPayload>(output.clone())
        {
            return Ok(parsed_payload_to_result(payload));
        }

        let raw = terminal_summary(&event.payload).unwrap_or_else(|| event.payload.to_string());
        match serde_json::from_str::<SubmitResultPayload>(&raw) {
            Ok(payload) => Ok(parsed_payload_to_result(payload)),
            Err(_) => Ok(ParsedRunResult {
                state: "completed".to_string(),
                summary: raw,
                outputs: Vec::new(),
                failure: None,
                signals: Vec::new(),
            }),
        }
    }
}
