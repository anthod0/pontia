use super::*;

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct CreateTaskRequest {
    pub input: String,
    pub workspace: Option<String>,
    #[serde(default = "default_client_type")]
    pub client_type: String,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CreateTaskOutcome {
    pub data: Value,
}

#[derive(Clone)]
pub struct TaskCommandService {
    pool: SqlitePool,
}

impl TaskCommandService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create_task(&self, request: CreateTaskRequest) -> Result<CreateTaskOutcome> {
        if !matches!(request.client_type.as_str(), "generic" | "pi") {
            return Err(Error::Domain(format!(
                "unsupported client_type: {}",
                request.client_type
            )));
        }

        let task_id = new_task_id().to_string();
        sqlx::query(
            r#"INSERT INTO tasks (task_id, state, input, routing_state, metadata)
               VALUES (?, 'created', ?, 'pending', ?)"#,
        )
        .bind(&task_id)
        .bind(&request.input)
        .bind(serde_json::to_string(&request.metadata)?)
        .execute(&self.pool)
        .await?;
        self.record_task_event(&task_id, "task.created", json!({}))
            .await?;

        let Some(workspace) = request.workspace.as_deref() else {
            sqlx::query(
                r#"UPDATE tasks
                   SET state = 'needs_confirmation', routing_state = 'ambiguous',
                       routing_reason = 'workspace is required until automatic routing is implemented',
                       updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                   WHERE task_id = ?"#,
            )
            .bind(&task_id)
            .execute(&self.pool)
            .await?;
            self.record_task_event(
                &task_id,
                "task.routing_ambiguous",
                json!({"reason":"workspace is required until automatic routing is implemented"}),
            )
            .await?;
            let task = ExternalQueryService::new(self.pool.clone())
                .get_task(&task_id)
                .await?
                .ok_or_else(|| Error::Domain("created task missing".to_string()))?;
            return Ok(CreateTaskOutcome {
                data: json!({ "task": task }),
            });
        };

        sqlx::query(
            r#"UPDATE tasks
               SET state = 'routing', updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE task_id = ?"#,
        )
        .bind(&task_id)
        .execute(&self.pool)
        .await?;
        self.record_task_event(&task_id, "task.routing_started", json!({}))
            .await?;

        let workspace_record = upsert_workspace(&self.pool, workspace).await?;
        sqlx::query(
            r#"UPDATE tasks
               SET workspace_id = ?, routing_state = 'matched', routing_confidence = 1.0,
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE task_id = ?"#,
        )
        .bind(&workspace_record.workspace_id)
        .bind(&task_id)
        .execute(&self.pool)
        .await?;
        self.record_task_event(
            &task_id,
            "task.workspace_matched",
            json!({"workspace_id": workspace_record.workspace_id, "canonical_path": workspace_record.canonical_path}),
        )
        .await?;

        let session_id = self
            .find_idle_session(&workspace_record.workspace_id, &request.client_type)
            .await?;
        let session_id = if let Some(session_id) = session_id {
            self.record_task_event(
                &task_id,
                "task.session_selected",
                json!({"session_id": session_id}),
            )
            .await?;
            session_id
        } else {
            let session_outcome = SessionCommandService::new(self.pool.clone())
                .create_session(
                    CreateSessionRequest {
                        client_type: request.client_type.clone(),
                        workspace: Some(workspace_record.canonical_path.clone()),
                        metadata: json!({"created_for_task_id": task_id}),
                        initial_task: None,
                    },
                    None,
                )
                .await?;
            let session_id = session_outcome.data["session"]["session_id"]
                .as_str()
                .ok_or_else(|| {
                    Error::Domain("created session response missing session_id".to_string())
                })?
                .to_string();
            self.record_task_event(
                &task_id,
                "task.session_created",
                json!({"session_id": session_id}),
            )
            .await?;
            session_id
        };

        sqlx::query(
            r#"UPDATE tasks
               SET state = 'queued', session_id = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE task_id = ?"#,
        )
        .bind(&session_id)
        .bind(&task_id)
        .execute(&self.pool)
        .await?;

        let turn_outcome = TurnCommandService::new(self.pool.clone())
            .submit_turn(
                &session_id,
                SubmitTurnRequest {
                    input: request.input,
                    metadata: request.metadata,
                },
                None,
            )
            .await?;
        let turn_id = turn_outcome.data["turn"]["turn_id"]
            .as_str()
            .ok_or_else(|| Error::Domain("created turn response missing turn_id".to_string()))?
            .to_string();
        let turn_state = turn_outcome.data["turn"]["state"]
            .as_str()
            .unwrap_or("queued");
        let task_state = if turn_state == "running" {
            "running"
        } else {
            "queued"
        };
        sqlx::query(
            r#"UPDATE tasks
               SET state = ?, turn_id = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE task_id = ?"#,
        )
        .bind(task_state)
        .bind(&turn_id)
        .bind(&task_id)
        .execute(&self.pool)
        .await?;
        self.record_task_event(&task_id, "task.turn_created", json!({"turn_id": turn_id}))
            .await?;

        let task = ExternalQueryService::new(self.pool.clone())
            .get_task(&task_id)
            .await?
            .ok_or_else(|| Error::Domain("created task missing".to_string()))?;
        Ok(CreateTaskOutcome {
            data: json!({ "task": task }),
        })
    }

    async fn find_idle_session(
        &self,
        workspace_id: &str,
        client_type: &str,
    ) -> Result<Option<String>> {
        sqlx::query_scalar(
            r#"SELECT session_id FROM sessions
               WHERE workspace_id = ? AND client_type = ? AND state IN ('idle', 'interrupted')
                 AND current_turn_id IS NULL
               ORDER BY updated_at DESC, session_id LIMIT 1"#,
        )
        .bind(workspace_id)
        .bind(client_type)
        .fetch_optional(&self.pool)
        .await
        .map_err(Into::into)
    }

    async fn record_task_event(
        &self,
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
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
