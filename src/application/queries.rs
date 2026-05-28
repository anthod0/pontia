use super::*;

#[derive(Clone)]
pub struct ExternalQueryService {
    pool: SqlitePool,
    graph: GraphRuntimeConfig,
}

impl ExternalQueryService {
    pub fn new(pool: SqlitePool) -> Self {
        Self::with_graph(pool, GraphRuntimeConfig::default())
    }

    pub fn with_graph(pool: SqlitePool, graph: GraphRuntimeConfig) -> Self {
        Self { graph, pool }
    }

    pub async fn list_sessions(&self) -> Result<Vec<SessionView>> {
        let rows = sqlx::query(
            r#"SELECT s.session_id, s.client_type, s.handle, s.role, s.description,
                      s.execution_profile_id, s.execution_profile_version,
                      s.state, s.current_turn_id, s.workspace_id,
                      COALESCE(w.canonical_path, s.workspace_ref) AS workspace_ref,
                      s.metadata, s.created_at, s.updated_at
               FROM sessions s
               LEFT JOIN workspaces w ON w.workspace_id = s.workspace_id
               ORDER BY s.created_at, s.session_id"#,
        )
        .fetch_all(&self.pool)
        .await?;

        let mut sessions = rows
            .into_iter()
            .map(row_to_session_view)
            .collect::<Result<Vec<_>>>()?;
        for session in &mut sessions {
            self.enrich_session_view(session).await?;
        }
        Ok(sessions)
    }

    pub async fn get_session(&self, session_id: &str) -> Result<Option<SessionView>> {
        let row = sqlx::query(
            r#"SELECT s.session_id, s.client_type, s.handle, s.role, s.description,
                      s.execution_profile_id, s.execution_profile_version,
                      s.state, s.current_turn_id, s.workspace_id,
                      COALESCE(w.canonical_path, s.workspace_ref) AS workspace_ref,
                      s.metadata, s.created_at, s.updated_at
               FROM sessions s
               LEFT JOIN workspaces w ON w.workspace_id = s.workspace_id
               WHERE s.session_id = ?"#,
        )
        .bind(session_id)
        .fetch_optional(&self.pool)
        .await?;

        let Some(row) = row else {
            return Ok(None);
        };
        let mut session = row_to_session_view(row)?;
        self.enrich_session_view(&mut session).await?;
        Ok(Some(session))
    }

    pub async fn list_workspaces(&self) -> Result<Vec<WorkspaceView>> {
        let rows = sqlx::query(
            r#"SELECT workspace_id, canonical_path, display_path, name, state, metadata,
                      created_at, updated_at, last_used_at
               FROM workspaces
               WHERE state != 'deleted'
               ORDER BY last_used_at DESC, created_at DESC, workspace_id"#,
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(row_to_workspace_view).collect()
    }

    pub async fn get_workspace(&self, workspace_id: &str) -> Result<Option<WorkspaceView>> {
        let row = sqlx::query(
            r#"SELECT workspace_id, canonical_path, display_path, name, state, metadata,
                      created_at, updated_at, last_used_at
               FROM workspaces WHERE workspace_id = ?"#,
        )
        .bind(workspace_id)
        .fetch_optional(&self.pool)
        .await?;

        row.map(row_to_workspace_view).transpose()
    }

    pub async fn list_tasks(&self) -> Result<Vec<TaskView>> {
        let rows = sqlx::query(
            r#"SELECT task_id, state, input, workspace_id, session_id, turn_id,
                      routing_state, routing_reason, routing_confidence, metadata,
                      created_at, updated_at
               FROM tasks ORDER BY created_at DESC, task_id"#,
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(row_to_task_view).collect()
    }

    pub async fn get_task(&self, task_id: &str) -> Result<Option<TaskView>> {
        let row = sqlx::query(
            r#"SELECT task_id, state, input, workspace_id, session_id, turn_id,
                      routing_state, routing_reason, routing_confidence, metadata,
                      created_at, updated_at
               FROM tasks WHERE task_id = ?"#,
        )
        .bind(task_id)
        .fetch_optional(&self.pool)
        .await?;

        row.map(row_to_task_view).transpose()
    }

    pub async fn list_task_events(&self, task_id: &str) -> Result<Vec<TaskEventView>> {
        let rows = sqlx::query(
            r#"SELECT event_id, task_id, event_type, payload, created_at
               FROM task_events WHERE task_id = ? ORDER BY created_at, event_id"#,
        )
        .bind(task_id)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(row_to_task_event_view).collect()
    }

    pub async fn list_task_dag_proposals(&self, task_id: &str) -> Result<Vec<DagProposalView>> {
        let rows = sqlx::query(
            r#"SELECT proposal_id, task_id, mode, state, summary, proposal_json,
                      validation_json, created_by_session_id, revision,
                      supersedes_proposal_id, created_at, updated_at
               FROM dag_proposals
               WHERE task_id = ?
               ORDER BY revision DESC, created_at DESC, proposal_id DESC"#,
        )
        .bind(task_id)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(row_to_dag_proposal_view).collect()
    }

    pub async fn get_task_dag(&self, task_id: &str) -> Result<TaskDagView> {
        let summary = self.get_task_dag_summary(task_id).await?;
        let work_items = self.list_work_items(task_id).await?;
        let edges = self.list_work_item_edges(task_id).await?;
        let runs = self.list_work_item_runs(task_id).await?;
        let signals = self.list_dag_signals(task_id).await?;
        Ok(TaskDagView {
            task_id: task_id.to_string(),
            summary,
            work_items,
            edges,
            runs,
            signals,
        })
    }

    pub async fn get_task_dag_summary(&self, task_id: &str) -> Result<DagSummaryView> {
        let graph = self.task_graph_snapshot(task_id).await?;
        let runtime = self.runtime_map(task_id).await?;
        let active_ids: std::collections::HashSet<_> = graph
            .work_items
            .iter()
            .filter(|work_item| work_item.active)
            .map(|work_item| work_item.work_item_id.as_str())
            .collect();

        let mut summary = DagSummaryView {
            total_work_items: active_ids.len() as i64,
            ready_work_items: 0,
            running_work_items: 0,
            completed_work_items: 0,
            blocked_work_items: 0,
            failed_work_items: 0,
            open_signals: 0,
            total_runs: 0,
        };

        for (work_item_id, runtime) in &runtime {
            if !active_ids.contains(work_item_id.as_str()) {
                continue;
            }
            match runtime.current_state.as_str() {
                "ready" => summary.ready_work_items += 1,
                "running" => summary.running_work_items += 1,
                "completed" => summary.completed_work_items += 1,
                "blocked" | "needs_input" => summary.blocked_work_items += 1,
                "failed" => summary.failed_work_items += 1,
                _ => {}
            }
        }

        summary.open_signals = self.count_open_signals(task_id).await?;
        summary.total_runs = self.count_work_item_runs(task_id).await?;
        Ok(summary)
    }

    pub async fn list_work_items(&self, task_id: &str) -> Result<Vec<WorkItemWithRuntimeView>> {
        let graph = self.task_graph_snapshot(task_id).await?;
        let runtime = self.runtime_map(task_id).await?;
        Ok(graph
            .work_items
            .into_iter()
            .map(|node| {
                let runtime = runtime.get(&node.work_item_id).cloned();
                WorkItemWithRuntimeView {
                    work_item: work_item_node_to_record(node),
                    runtime,
                }
            })
            .collect())
    }

    pub async fn list_work_item_edges(&self, task_id: &str) -> Result<Vec<WorkItemEdgeView>> {
        Ok(self
            .task_graph_snapshot(task_id)
            .await?
            .edges
            .into_iter()
            .map(graph_edge_record_to_view)
            .collect())
    }

    pub async fn list_work_item_runs(&self, task_id: &str) -> Result<Vec<WorkItemRunRecord>> {
        let rows = sqlx::query(
            r#"SELECT run_id, work_item_id, task_id, attempt, state, session_id, turn_id,
                      client_type, execution_profile_id, execution_profile_version,
                      rendered_prompt_ref, output_summary, failure, created_at, updated_at,
                      started_at, completed_at
               FROM work_item_runs WHERE task_id = ? ORDER BY created_at, run_id"#,
        )
        .bind(task_id)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(row_to_work_item_run_record).collect()
    }

    pub async fn list_dag_signals(&self, task_id: &str) -> Result<Vec<DagSignalRecord>> {
        let rows = sqlx::query(
            r#"SELECT signal_id, task_id, work_item_id, run_id, source_session_id, source, kind,
                      summary, detail, severity, related_refs, state, created_at, updated_at
               FROM dag_signals WHERE task_id = ? ORDER BY created_at, signal_id"#,
        )
        .bind(task_id)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(row_to_dag_signal_record).collect()
    }

    pub async fn list_relevant_dag_proposals(&self, task_id: &str) -> Result<Vec<DagProposal>> {
        let rows = sqlx::query(
            r#"SELECT proposal_id, task_id, mode, state, summary, proposal_json,
                      validation_json, created_by_session_id, revision,
                      supersedes_proposal_id, created_at, updated_at
               FROM dag_proposals
               WHERE task_id = ? AND state IN ('proposed', 'validated', 'rejected', 'superseded')
               ORDER BY revision DESC, created_at DESC, proposal_id"#,
        )
        .bind(task_id)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(row_to_dag_proposal).collect()
    }

    pub async fn list_turns(&self, session_id: &str) -> Result<Vec<TurnView>> {
        let rows = sqlx::query(
            r#"SELECT turn_id, session_id, state, input_summary, output_summary,
                      failure_message, metadata, created_at, updated_at
               FROM turns WHERE session_id = ? ORDER BY created_at, turn_id"#,
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?;

        let mut turns = rows
            .into_iter()
            .map(row_to_turn_view)
            .collect::<Result<Vec<_>>>()?;
        for turn in &mut turns {
            self.enrich_turn_view(turn).await?;
        }
        Ok(turns)
    }

    pub async fn get_turn(&self, session_id: &str, turn_id: &str) -> Result<Option<TurnView>> {
        let row = sqlx::query(
            r#"SELECT turn_id, session_id, state, input_summary, output_summary,
                      failure_message, metadata, created_at, updated_at
               FROM turns WHERE session_id = ? AND turn_id = ?"#,
        )
        .bind(session_id)
        .bind(turn_id)
        .fetch_optional(&self.pool)
        .await?;

        let Some(row) = row else {
            return Ok(None);
        };
        let mut turn = row_to_turn_view(row)?;
        self.enrich_turn_view(&mut turn).await?;
        Ok(Some(turn))
    }

    pub async fn list_session_events(&self, session_id: &str) -> Result<Vec<EventView>> {
        let rows = sqlx::query(
            r#"SELECT event_id, session_id, turn_id, source, event_type, occurred_at, payload
               FROM events WHERE session_id = ? ORDER BY rowid"#,
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(row_to_event_view).collect()
    }

    pub async fn list_turn_events(
        &self,
        session_id: &str,
        turn_id: &str,
    ) -> Result<Vec<EventView>> {
        let rows = sqlx::query(
            r#"SELECT event_id, session_id, turn_id, source, event_type, occurred_at, payload
               FROM events WHERE session_id = ? AND turn_id = ? ORDER BY rowid"#,
        )
        .bind(session_id)
        .bind(turn_id)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(row_to_event_view).collect()
    }

    pub async fn resolve_event_cursor(
        &self,
        scope: EventStreamScope<'_>,
        after_event_id: &str,
    ) -> Result<i64> {
        let row = match scope {
            EventStreamScope::Session { session_id } => {
                sqlx::query("SELECT rowid FROM events WHERE session_id = ? AND event_id = ?")
                    .bind(session_id)
                    .bind(after_event_id)
                    .fetch_optional(&self.pool)
                    .await?
            }
            EventStreamScope::Turn {
                session_id,
                turn_id,
            } => sqlx::query(
                "SELECT rowid FROM events WHERE session_id = ? AND turn_id = ? AND event_id = ?",
            )
            .bind(session_id)
            .bind(turn_id)
            .bind(after_event_id)
            .fetch_optional(&self.pool)
            .await?,
        };

        let Some(row) = row else {
            return Err(Error::Domain(format!(
                "event cursor {after_event_id} is not valid for requested stream"
            )));
        };

        Ok(row.try_get("rowid")?)
    }

    pub fn parse_dashboard_stream_cursor(&self, cursor: &str) -> Result<DashboardStreamCursor> {
        let mut session_rowid = None;
        let mut task_rowid = None;
        for part in cursor.split(';') {
            let Some((name, value)) = part.split_once(':') else {
                return Err(Error::Domain(format!(
                    "dashboard cursor {cursor} is invalid"
                )));
            };
            let rowid = value
                .parse::<i64>()
                .map_err(|_| Error::Domain(format!("dashboard cursor {cursor} is invalid")))?;
            match name {
                "session" => session_rowid = Some(rowid),
                "task" => task_rowid = Some(rowid),
                _ => {
                    return Err(Error::Domain(format!(
                        "dashboard cursor {cursor} is invalid"
                    )));
                }
            }
        }
        Ok(DashboardStreamCursor {
            session_rowid: session_rowid.unwrap_or(0),
            task_rowid: task_rowid.unwrap_or(0),
        })
    }

    pub async fn list_dashboard_stream_items_after(
        &self,
        cursor: DashboardStreamCursor,
        limit: i64,
    ) -> Result<Vec<DashboardStreamItem>> {
        let session_rows = sqlx::query(
            r#"SELECT rowid, event_id, session_id, turn_id, source, event_type, occurred_at, payload
               FROM events WHERE rowid > ? ORDER BY rowid LIMIT ?"#,
        )
        .bind(cursor.session_rowid)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        let task_rows = sqlx::query(
            r#"SELECT rowid, event_id, task_id, event_type, payload, created_at
               FROM task_events WHERE rowid > ? ORDER BY rowid LIMIT ?"#,
        )
        .bind(cursor.task_rowid)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        let mut items = Vec::new();
        for row in session_rows {
            let rowid = row.try_get("rowid")?;
            let event = row_to_event_view(row)?;
            let occurred_at = event.time.clone();
            items.push(DashboardStreamItem {
                cursor: DashboardStreamCursor {
                    session_rowid: rowid,
                    task_rowid: cursor.task_rowid,
                },
                occurred_at: occurred_at.clone(),
                event: DashboardStreamEvent::SessionEvent {
                    id: event.event_id.clone(),
                    occurred_at,
                    event,
                },
            });
        }
        for row in task_rows {
            let rowid = row.try_get("rowid")?;
            let event = row_to_task_event_view(row)?;
            let occurred_at = event.created_at.clone();
            items.push(DashboardStreamItem {
                cursor: DashboardStreamCursor {
                    session_rowid: cursor.session_rowid,
                    task_rowid: rowid,
                },
                occurred_at: occurred_at.clone(),
                event: DashboardStreamEvent::TaskEvent {
                    id: event.event_id.clone(),
                    occurred_at,
                    event,
                },
            });
        }

        items.sort_by(|a, b| {
            a.occurred_at
                .cmp(&b.occurred_at)
                .then(a.cursor.session_rowid.cmp(&b.cursor.session_rowid))
                .then(a.cursor.task_rowid.cmp(&b.cursor.task_rowid))
        });
        items.truncate(limit as usize);
        let mut running = cursor;
        for item in &mut items {
            running.session_rowid = running.session_rowid.max(item.cursor.session_rowid);
            running.task_rowid = running.task_rowid.max(item.cursor.task_rowid);
            item.cursor = running;
        }
        Ok(items)
    }

    pub async fn list_event_stream_items_after(
        &self,
        scope: EventStreamScope<'_>,
        after_rowid: i64,
        limit: i64,
    ) -> Result<Vec<EventStreamItem>> {
        let rows = match scope {
            EventStreamScope::Session { session_id } => {
                sqlx::query(
                    r#"SELECT rowid, event_id, session_id, turn_id, source, event_type, occurred_at, payload
                       FROM events WHERE session_id = ? AND rowid > ? ORDER BY rowid LIMIT ?"#,
                )
                .bind(session_id)
                .bind(after_rowid)
                .bind(limit)
                .fetch_all(&self.pool)
                .await?
            }
            EventStreamScope::Turn {
                session_id,
                turn_id,
            } => {
                sqlx::query(
                    r#"SELECT rowid, event_id, session_id, turn_id, source, event_type, occurred_at, payload
                       FROM events WHERE session_id = ? AND turn_id = ? AND rowid > ? ORDER BY rowid LIMIT ?"#,
                )
                .bind(session_id)
                .bind(turn_id)
                .bind(after_rowid)
                .bind(limit)
                .fetch_all(&self.pool)
                .await?
            }
        };

        rows.into_iter().map(row_to_event_stream_item).collect()
    }

    async fn runtime_map(
        &self,
        task_id: &str,
    ) -> Result<std::collections::HashMap<String, WorkItemRuntimeView>> {
        let rows = sqlx::query(
            r#"SELECT work_item_id, current_run_id, current_state, current_attempt, ready_at,
                      blocked_reason, outcome_state, outcome_reason, replanned_from_state,
                      retry_count, max_retries, priority, optional,
                      parallelizable, session_id, turn_id, updated_at
               FROM work_item_runtime_projection
               WHERE task_id = ?"#,
        )
        .bind(task_id)
        .fetch_all(&self.pool)
        .await?;

        let mut runtime = std::collections::HashMap::new();
        for row in rows {
            runtime.insert(
                row.try_get("work_item_id")?,
                WorkItemRuntimeView {
                    current_run_id: row.try_get("current_run_id")?,
                    current_state: row.try_get("current_state")?,
                    current_attempt: row.try_get("current_attempt")?,
                    ready_at: row.try_get("ready_at")?,
                    blocked_reason: row.try_get("blocked_reason")?,
                    outcome_state: row.try_get("outcome_state")?,
                    outcome_reason: row.try_get("outcome_reason")?,
                    replanned_from_state: row.try_get("replanned_from_state")?,
                    retry_count: row.try_get("retry_count")?,
                    max_retries: row.try_get("max_retries")?,
                    priority: row.try_get("priority")?,
                    optional: row.try_get("optional")?,
                    parallelizable: row.try_get("parallelizable")?,
                    session_id: row.try_get("session_id")?,
                    turn_id: row.try_get("turn_id")?,
                    updated_at: row.try_get("updated_at")?,
                },
            );
        }
        Ok(runtime)
    }

    async fn count_open_signals(&self, task_id: &str) -> Result<i64> {
        Ok(sqlx::query_scalar(
            "SELECT COUNT(*) FROM dag_signals WHERE task_id = ? AND state = 'open'",
        )
        .bind(task_id)
        .fetch_one(&self.pool)
        .await?)
    }

    async fn count_work_item_runs(&self, task_id: &str) -> Result<i64> {
        Ok(
            sqlx::query_scalar("SELECT COUNT(*) FROM work_item_runs WHERE task_id = ?")
                .bind(task_id)
                .fetch_one(&self.pool)
                .await?,
        )
    }

    pub async fn list_artifacts(&self, session_id: &str) -> Result<Vec<ArtifactView>> {
        let rows = sqlx::query(
            r#"SELECT artifact_id, session_id, turn_id, kind, name, size_bytes, metadata, created_at
               FROM artifacts WHERE session_id = ? ORDER BY created_at, artifact_id"#,
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(row_to_artifact_view).collect()
    }

    async fn enrich_session_view(&self, session: &mut SessionView) -> Result<()> {
        let row = sqlx::query("SELECT metadata FROM runtime_bindings WHERE session_id = ?")
            .bind(&session.session_id)
            .fetch_optional(&self.pool)
            .await?;

        if let Some(row) = row {
            let metadata: String = row.try_get("metadata")?;
            let metadata: Value = serde_json::from_str(&metadata)?;
            if let Some(capabilities) = metadata.get("capabilities") {
                session.capabilities = serde_json::from_value(capabilities.clone())?;
            }
        }

        Ok(())
    }

    pub(crate) async fn enrich_turn_view(&self, turn: &mut TurnView) -> Result<()> {
        let rows = sqlx::query(
            r#"SELECT event_type, occurred_at, payload
               FROM events WHERE session_id = ? AND turn_id = ? ORDER BY rowid"#,
        )
        .bind(&turn.session_id)
        .bind(&turn.turn_id)
        .fetch_all(&self.pool)
        .await?;

        for row in rows {
            let event_type: String = row.try_get("event_type")?;
            let occurred_at: String = row.try_get("occurred_at")?;
            let payload: String = row.try_get("payload")?;
            let payload: Value = serde_json::from_str(&payload)?;

            match event_type.as_str() {
                "turn.created" | "turn.queued" | "turn.started" => {
                    if event_type == "turn.started" && turn.started_at.is_none() {
                        turn.started_at = Some(occurred_at.clone());
                    }
                    if turn.input.summary.is_none() {
                        turn.input.summary = nested_string(&payload, &["input", "summary"])
                            .or_else(|| nested_string(&payload, &["input_summary"]));
                    }
                    if turn.input.artifact_id.is_none() {
                        turn.input.artifact_id = nested_string(&payload, &["input", "artifact_id"])
                            .or_else(|| nested_string(&payload, &["input_artifact_id"]));
                    }
                }
                "turn.output" | "turn.completed" => {
                    if event_type == "turn.completed" && turn.state != "completed" {
                        continue;
                    }
                    if event_type == "turn.completed" {
                        turn.completed_at = Some(occurred_at.clone());
                    }
                    if turn.output.summary.is_none() {
                        turn.output.summary = nested_string(&payload, &["output", "summary"])
                            .or_else(|| nested_string(&payload, &["output_summary"]));
                    }
                    if turn.output.artifact_ids.is_empty()
                        && let Some(ids) =
                            nested_array_strings(&payload, &["output", "artifact_ids"])
                                .or_else(|| nested_array_strings(&payload, &["artifact_ids"]))
                    {
                        turn.output.artifact_ids = ids;
                    }
                    if event_type == "turn.completed" {
                        break;
                    }
                }
                "turn.failed" | "turn.interrupted" | "turn.cancelled" => {
                    let expected_state = event_type.strip_prefix("turn.").unwrap_or_default();
                    if turn.state != expected_state {
                        continue;
                    }
                    turn.completed_at = Some(occurred_at);
                    if turn.failure.is_none() {
                        turn.failure = nested_string(&payload, &["failure", "message"])
                            .or_else(|| nested_string(&payload, &["message"]));
                    }
                    break;
                }
                _ => {}
            }
        }

        Ok(())
    }

    pub async fn get_artifact(&self, artifact_id: &str) -> Result<Option<ArtifactView>> {
        let row = sqlx::query(
            r#"SELECT artifact_id, session_id, turn_id, kind, name, size_bytes, metadata, created_at
               FROM artifacts WHERE artifact_id = ?"#,
        )
        .bind(artifact_id)
        .fetch_optional(&self.pool)
        .await?;

        row.map(row_to_artifact_view).transpose()
    }

    async fn task_graph_snapshot(&self, task_id: &str) -> Result<TaskGraphSnapshot> {
        GraphProjectionService::new(self.pool.clone(), self.graph.clone())
            .task_graph(task_id)
            .await
    }
}
