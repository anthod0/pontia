use super::*;

#[derive(Debug, Clone)]
struct RunForTurn {
    run_id: String,
    work_item_id: String,
    task_id: String,
    session_id: Option<String>,
    state: String,
}

#[derive(Debug, Clone)]
struct ParsedRunResult {
    state: String,
    summary: String,
    outputs: Vec<Value>,
    failure: Option<Value>,
    signals: Vec<RaiseSignalPayload>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SubmitResultToolOutcome {
    pub task_id: String,
    pub work_item_id: String,
    pub run_id: String,
    pub state: String,
    pub scheduler: DagSchedulerOutcome,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RaiseSignalToolOutcome {
    pub signal_id: String,
    pub task_id: String,
    pub work_item_id: Option<String>,
    pub run_id: Option<String>,
    pub kind: String,
    pub state: String,
    pub replanner_started: bool,
}

struct TerminalEventRefs {
    turn_id: Option<String>,
    domain_event_id: Option<String>,
}

struct SignalEvent<'a> {
    task_id: &'a str,
    signal_id: &'a str,
    work_item_id: Option<&'a str>,
    run_id: Option<&'a str>,
    source_session_id: Option<&'a str>,
    source: &'a str,
    payload: &'a RaiseSignalPayload,
}

#[derive(Clone)]
pub struct DagRunResultService {
    pool: SqlitePool,
}

impl DagRunResultService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

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
                DagPlanningService::new(self.pool.clone())
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

    async fn run_for_tool_context(&self, context: &AgentToolContext) -> Result<RunForTurn> {
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

    async fn run_for_turn(&self, turn_id: &str) -> Result<Option<RunForTurn>> {
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

    async fn mark_started(&self, run: &RunForTurn) -> Result<()> {
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
                       updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                   WHERE work_item_id = ? AND current_run_id = ?"#,
            )
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
                    DagPlanningService::new(self.pool.clone())
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
            Box::pin(DagSchedulerService::new(self.pool.clone()).schedule_task(&run.task_id))
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

    async fn terminate_run_session(&self, run: &RunForTurn) -> Result<()> {
        if let Some(session_id) = run.session_id.as_deref() {
            Box::pin(
                RuntimeControlService::new(self.pool.clone()).terminate_session(session_id, None),
            )
            .await?;
        }
        Ok(())
    }

    async fn aggregate_task_state(&self, task_id: &str) -> Result<()> {
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

    async fn emit_signal_event(&self, event: SignalEvent<'_>) -> Result<()> {
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
        GraphProjectionService::new(self.pool.clone(), GraphRuntimeConfig::default())
            .project_task(event.task_id)
            .await
    }

    async fn record_task_event(
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

fn terminal_summary(payload: &Value) -> Option<String> {
    nested_string(payload, &["output", "summary"])
        .or_else(|| nested_string(payload, &["output_summary"]))
        .or_else(|| nested_string(payload, &["summary"]))
        .or_else(|| nested_string(payload, &["output", "text"]))
        .or_else(|| nested_string(payload, &["output", "content"]))
        .or_else(|| {
            payload
                .get("output")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
}

fn failure_summary(payload: &Value) -> String {
    nested_string(payload, &["failure", "message"])
        .or_else(|| nested_string(payload, &["message"]))
        .unwrap_or_else(|| "turn failed".to_string())
}

fn parsed_payload_to_result(payload: SubmitResultPayload) -> ParsedRunResult {
    ParsedRunResult {
        state: normalize_result_status(&payload.status),
        summary: payload.summary,
        outputs: payload.outputs,
        failure: payload.failure,
        signals: payload.signals,
    }
}

fn validate_result_status(status: &str) -> Result<()> {
    match status {
        "completed" | "failed" | "blocked" | "needs_input" => Ok(()),
        other => Err(Error::Domain(format!(
            "submitResult status must be completed, failed, blocked, or needs_input, got {other}"
        ))),
    }
}

fn normalize_result_status(status: &str) -> String {
    match status {
        "completed" | "failed" | "blocked" | "needs_input" => status.to_string(),
        _ => "completed".to_string(),
    }
}

fn signal_blocking_state(kind: &str) -> &'static str {
    match kind {
        "needs_input" | "assistance_needed" => "needs_input",
        _ => "blocked",
    }
}

fn signal_projection_state(kind: &str) -> &'static str {
    match kind {
        "replan_requested" => "replan_anchor",
        _ => signal_blocking_state(kind),
    }
}

fn validate_signal_kind(kind: &str) -> Result<()> {
    match kind {
        "needs_input" | "replan_requested" | "risk" | "missing_dependency" | "scope_change"
        | "assistance_needed" | "review_requested" | "other" => Ok(()),
        other => Err(Error::Domain(format!(
            "raiseSignal kind is not supported: {other}"
        ))),
    }
}

fn normalize_severity(severity: &str) -> &str {
    match severity {
        "low" | "medium" | "high" => severity,
        _ => "medium",
    }
}

fn is_terminal_run_state(state: &str) -> bool {
    matches!(
        state,
        "completed" | "failed" | "blocked" | "needs_input" | "cancelled"
    )
}

fn new_dag_run_result_id(prefix: &str) -> String {
    format!("{prefix}_{}", uuid::Uuid::now_v7())
}
