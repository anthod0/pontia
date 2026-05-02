use super::*;

#[derive(Clone)]
pub struct ExternalQueryService {
    pool: SqlitePool,
}

impl ExternalQueryService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn list_sessions(&self) -> Result<Vec<SessionView>> {
        let rows = sqlx::query(
            r#"SELECT s.session_id, s.client_type, s.state, s.current_turn_id, s.workspace_id,
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
            r#"SELECT s.session_id, s.client_type, s.state, s.current_turn_id, s.workspace_id,
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
               FROM workspaces ORDER BY last_used_at DESC, created_at DESC, workspace_id"#,
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(row_to_workspace_view).collect()
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
}
