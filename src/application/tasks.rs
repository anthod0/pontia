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

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ConfirmTaskWorkspaceRequest {
    pub workspace: String,
    #[serde(default = "default_client_type")]
    pub client_type: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CreateTaskOutcome {
    pub data: Value,
    pub duplicate: bool,
}

#[derive(Clone)]
pub struct TaskCommandService {
    pool: SqlitePool,
    planner: PlannerRuntimeConfig,
}

impl TaskCommandService {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            planner: PlannerRuntimeConfig::default(),
        }
    }

    pub fn with_planner(pool: SqlitePool, planner: PlannerRuntimeConfig) -> Self {
        Self { pool, planner }
    }

    pub async fn create_task(
        &self,
        request: CreateTaskRequest,
        idempotency_key: Option<&str>,
    ) -> Result<CreateTaskOutcome> {
        if !matches!(request.client_type.as_str(), "generic" | "pi") {
            return Err(Error::Domain(format!(
                "unsupported client_type: {}",
                request.client_type
            )));
        }

        if let Some(key) = idempotency_key
            && let Some(response) = self.idempotency_response("create_task", key).await?
        {
            return Ok(CreateTaskOutcome {
                data: response,
                duplicate: true,
            });
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
            if self.planner.enabled {
                let data = self.run_initial_planner_attempt(&task_id, &request).await?;
                if let Some(key) = idempotency_key {
                    self.store_idempotency_response("create_task", key, &data)
                        .await?;
                }
                return Ok(CreateTaskOutcome {
                    data,
                    duplicate: false,
                });
            }

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
            let data = json!({ "task": task });
            if let Some(key) = idempotency_key {
                self.store_idempotency_response("create_task", key, &data)
                    .await?;
            }
            return Ok(CreateTaskOutcome {
                data,
                duplicate: false,
            });
        };

        if let Err(error) = self
            .dispatch_task(
                &task_id,
                workspace,
                &request.client_type,
                request.input,
                request.metadata,
                DispatchRoutingUpdate::Matched,
            )
            .await
        {
            self.mark_task_failed(&task_id, &error.to_string()).await?;
            return Err(error);
        }

        let task = ExternalQueryService::new(self.pool.clone())
            .get_task(&task_id)
            .await?
            .ok_or_else(|| Error::Domain("created task missing".to_string()))?;
        let data = json!({ "task": task });
        if let Some(key) = idempotency_key {
            self.store_idempotency_response("create_task", key, &data)
                .await?;
        }
        Ok(CreateTaskOutcome {
            data,
            duplicate: false,
        })
    }

    pub async fn submit_planner_input(
        &self,
        task_id: &str,
        request: SubmitPlannerInputRequest,
        idempotency_key: Option<&str>,
    ) -> Result<CreateTaskOutcome> {
        if let Some(key) = idempotency_key
            && let Some(response) = self
                .idempotency_response(&format!("planner_input:{task_id}"), key)
                .await?
        {
            return Ok(CreateTaskOutcome {
                data: response,
                duplicate: true,
            });
        }

        if !matches!(request.client_type.as_str(), "generic" | "pi") {
            return Err(Error::Domain(format!(
                "unsupported client_type: {}",
                request.client_type
            )));
        }
        if !self.planner.enabled {
            return Err(Error::StateConflict("planner is not enabled".to_string()));
        }

        let task = ExternalQueryService::new(self.pool.clone())
            .get_task(task_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("task {task_id} not found")))?;
        if task.turn_id.is_some() || is_terminal_task_state(&task.state) {
            return Err(Error::StateConflict(format!(
                "task {task_id} has already been dispatched or is terminal"
            )));
        }
        let can_resume = task.state == "needs_confirmation"
            || matches!(
                task.routing_state.as_str(),
                "ambiguous" | "failed" | "pending"
            );
        if !can_resume {
            return Err(Error::StateConflict(format!(
                "task {task_id} cannot receive planner input from state {}",
                task.state
            )));
        }

        self.record_task_event(
            task_id,
            "task.planning_input_received",
            json!({"message": request.message, "client_type": request.client_type}),
        )
        .await?;
        let planner = TaskPlannerService::new(self.pool.clone(), FakeTaskPlanner);
        let input = planner
            .build_input(task_id, task.input, task.metadata, None)
            .await?;
        let data = self
            .apply_planner_attempt(task_id, &request.client_type, input)
            .await?;
        if let Some(key) = idempotency_key {
            self.store_idempotency_response(&format!("planner_input:{task_id}"), key, &data)
                .await?;
        }
        Ok(CreateTaskOutcome {
            data,
            duplicate: false,
        })
    }

    pub async fn confirm_workspace(
        &self,
        task_id: &str,
        request: ConfirmTaskWorkspaceRequest,
        idempotency_key: Option<&str>,
    ) -> Result<CreateTaskOutcome> {
        if let Some(key) = idempotency_key
            && let Some(response) = self
                .idempotency_response(&format!("confirm_workspace:{task_id}"), key)
                .await?
        {
            return Ok(CreateTaskOutcome {
                data: response,
                duplicate: true,
            });
        }

        if !matches!(request.client_type.as_str(), "generic" | "pi") {
            return Err(Error::Domain(format!(
                "unsupported client_type: {}",
                request.client_type
            )));
        }

        let task = ExternalQueryService::new(self.pool.clone())
            .get_task(task_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("task {task_id} not found")))?;

        if task.turn_id.is_some() || is_terminal_task_state(&task.state) {
            return Err(Error::StateConflict(format!(
                "task {task_id} has already been dispatched or is terminal"
            )));
        }

        let can_confirm = task.state == "needs_confirmation"
            || matches!(task.routing_state.as_str(), "ambiguous" | "failed");
        if !can_confirm {
            return Err(Error::StateConflict(format!(
                "task {task_id} cannot be workspace-confirmed from state {}",
                task.state
            )));
        }

        if let Err(error) = self
            .dispatch_task(
                task_id,
                &request.workspace,
                &request.client_type,
                task.input,
                task.metadata,
                DispatchRoutingUpdate::Confirmed,
            )
            .await
        {
            self.mark_task_failed(task_id, &error.to_string()).await?;
            return Err(error);
        }

        let task = ExternalQueryService::new(self.pool.clone())
            .get_task(task_id)
            .await?
            .ok_or_else(|| Error::Domain("confirmed task missing".to_string()))?;
        let data = json!({ "task": task });
        if let Some(key) = idempotency_key {
            self.store_idempotency_response(&format!("confirm_workspace:{task_id}"), key, &data)
                .await?;
        }
        Ok(CreateTaskOutcome {
            data,
            duplicate: false,
        })
    }

    pub async fn interrupt_task(
        &self,
        task_id: &str,
        idempotency_key: Option<&str>,
    ) -> Result<CreateTaskOutcome> {
        if let Some(key) = idempotency_key
            && let Some(response) = self
                .idempotency_response(&format!("interrupt_task:{task_id}"), key)
                .await?
        {
            return Ok(CreateTaskOutcome {
                data: response,
                duplicate: true,
            });
        }

        let task = ExternalQueryService::new(self.pool.clone())
            .get_task(task_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("task {task_id} not found")))?;
        if is_terminal_task_state(&task.state) {
            return Err(Error::StateConflict(format!(
                "task {task_id} is already terminal"
            )));
        }
        let session_id = task.session_id.ok_or_else(|| {
            Error::StateConflict(format!("task {task_id} has no session to interrupt"))
        })?;
        let turn_id = task.turn_id.ok_or_else(|| {
            Error::StateConflict(format!("task {task_id} has no turn to interrupt"))
        })?;

        RuntimeControlService::new(self.pool.clone())
            .interrupt_turn(&session_id, &turn_id, idempotency_key)
            .await?;
        let task = ExternalQueryService::new(self.pool.clone())
            .get_task(task_id)
            .await?
            .ok_or_else(|| Error::Domain("interrupted task missing".to_string()))?;
        let data = json!({ "task": task });
        if let Some(key) = idempotency_key {
            self.store_idempotency_response(&format!("interrupt_task:{task_id}"), key, &data)
                .await?;
        }
        Ok(CreateTaskOutcome {
            data,
            duplicate: false,
        })
    }

    pub async fn cancel_task(
        &self,
        task_id: &str,
        idempotency_key: Option<&str>,
    ) -> Result<CreateTaskOutcome> {
        if let Some(key) = idempotency_key
            && let Some(response) = self
                .idempotency_response(&format!("cancel_task:{task_id}"), key)
                .await?
        {
            return Ok(CreateTaskOutcome {
                data: response,
                duplicate: true,
            });
        }

        let task = ExternalQueryService::new(self.pool.clone())
            .get_task(task_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("task {task_id} not found")))?;
        if is_terminal_task_state(&task.state) {
            return Err(Error::StateConflict(format!(
                "task {task_id} is already terminal"
            )));
        }

        if task.turn_id.is_some() {
            return self.interrupt_task(task_id, idempotency_key).await;
        }

        sqlx::query(
            r#"UPDATE tasks
               SET state = 'cancelled', updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE task_id = ?"#,
        )
        .bind(task_id)
        .execute(&self.pool)
        .await?;
        self.record_task_event(
            task_id,
            "task.cancelled",
            json!({"reason":"cancelled by user"}),
        )
        .await?;
        let task = ExternalQueryService::new(self.pool.clone())
            .get_task(task_id)
            .await?
            .ok_or_else(|| Error::Domain("cancelled task missing".to_string()))?;
        let data = json!({ "task": task });
        if let Some(key) = idempotency_key {
            self.store_idempotency_response(&format!("cancel_task:{task_id}"), key, &data)
                .await?;
        }
        Ok(CreateTaskOutcome {
            data,
            duplicate: false,
        })
    }

    async fn run_initial_planner_attempt(
        &self,
        task_id: &str,
        request: &CreateTaskRequest,
    ) -> Result<Value> {
        let planner = TaskPlannerService::new(self.pool.clone(), FakeTaskPlanner);
        let input = planner
            .build_input(
                task_id,
                request.input.clone(),
                request.metadata.clone(),
                None,
            )
            .await?;
        self.apply_planner_attempt(task_id, &request.client_type, input)
            .await
    }

    async fn apply_planner_attempt(
        &self,
        task_id: &str,
        client_type: &str,
        input: PlannerInput,
    ) -> Result<Value> {
        sqlx::query(
            r#"UPDATE tasks
               SET state = 'routing', routing_state = 'pending', routing_reason = NULL,
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE task_id = ?"#,
        )
        .bind(task_id)
        .execute(&self.pool)
        .await?;
        self.record_task_event(
            task_id,
            "task.planning_started",
            json!({"planner_client_type": self.planner.client_type}),
        )
        .await?;

        let decision = match self.plan_with_config(input).await {
            Ok(decision) => decision,
            Err(error) => {
                self.apply_planner_failed(task_id, &error.to_string(), None)
                    .await?;
                return self.task_data(task_id).await;
            }
        };

        self.record_task_event(
            task_id,
            "task.planning_completed",
            json!({"decision": decision}),
        )
        .await?;

        match decision.status {
            PlannerDecisionStatus::Resolved => {
                self.apply_planner_resolved(task_id, client_type, &decision)
                    .await?;
            }
            PlannerDecisionStatus::NeedsInput => {
                let question = decision
                    .needs_input
                    .as_ref()
                    .map(|needs_input| needs_input.question.clone())
                    .unwrap_or_else(|| "Planner needs more input".to_string());
                sqlx::query(
                    r#"UPDATE tasks
                       SET state = 'needs_confirmation', routing_state = 'ambiguous',
                           routing_reason = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                       WHERE task_id = ?"#,
                )
                .bind(&question)
                .bind(task_id)
                .execute(&self.pool)
                .await?;
                self.record_task_event(
                    task_id,
                    "task.planning_needs_input",
                    json!({"decision": decision, "question": question}),
                )
                .await?;
            }
            PlannerDecisionStatus::Failed => {
                let reason = decision
                    .reason
                    .as_deref()
                    .unwrap_or("planner failed")
                    .to_string();
                self.apply_planner_failed(task_id, &reason, Some(decision))
                    .await?;
            }
        }

        self.task_data(task_id).await
    }

    async fn plan_with_config(&self, input: PlannerInput) -> Result<PlannerDecision> {
        if self.planner.client_type == "pi" {
            TaskPlannerService::new(
                self.pool.clone(),
                PiTaskPlanner::new(std::time::Duration::from_millis(self.planner.timeout_ms)),
            )
            .plan(input)
            .await
        } else {
            TaskPlannerService::new(self.pool.clone(), FakeTaskPlanner)
                .plan(input)
                .await
        }
    }

    async fn apply_planner_resolved(
        &self,
        task_id: &str,
        client_type: &str,
        decision: &PlannerDecision,
    ) -> Result<()> {
        let workspace = decision.workspace.as_ref().ok_or_else(|| {
            Error::Domain("resolved planner decision missing workspace".to_string())
        })?;
        let workspace_record = if let Some(workspace_id) = workspace.workspace_id.as_deref() {
            let row = sqlx::query(
                "SELECT workspace_id, canonical_path FROM workspaces WHERE workspace_id = ?",
            )
            .bind(workspace_id)
            .fetch_optional(&self.pool)
            .await?;
            if let Some(row) = row {
                WorkspaceRecord {
                    workspace_id: row.try_get("workspace_id")?,
                    canonical_path: row.try_get("canonical_path")?,
                }
            } else if let Some(canonical_path) = workspace.canonical_path.as_deref() {
                upsert_workspace(&self.pool, canonical_path).await?
            } else {
                return Err(Error::Domain(format!(
                    "planner resolved unknown workspace_id {workspace_id}"
                )));
            }
        } else {
            upsert_workspace(
                &self.pool,
                workspace.canonical_path.as_deref().ok_or_else(|| {
                    Error::Domain("resolved planner decision missing canonical_path".to_string())
                })?,
            )
            .await?
        };

        let confidence = workspace.confidence.unwrap_or(1.0).clamp(0.0, 1.0);
        let reason = workspace
            .reason
            .as_deref()
            .or(decision.reason.as_deref())
            .unwrap_or("planner resolved workspace");
        sqlx::query(
            r#"UPDATE tasks
               SET state = 'routing', workspace_id = ?, routing_state = 'matched',
                   routing_confidence = ?, routing_reason = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE task_id = ?"#,
        )
        .bind(&workspace_record.workspace_id)
        .bind(confidence)
        .bind(reason)
        .bind(task_id)
        .execute(&self.pool)
        .await?;
        self.record_task_event(
            task_id,
            "task.planning_resolved",
            json!({"decision": decision, "workspace_id": workspace_record.workspace_id.clone(), "canonical_path": workspace_record.canonical_path.clone()}),
        )
        .await?;

        let handoff_id = format!("handoff_{}", uuid::Uuid::now_v7());
        self.record_task_event(
            task_id,
            "task.dispatch_handoff_created",
            json!({
                "handoff_id": handoff_id,
                "decision_id": decision.decision_id.clone(),
                "task_id": task_id,
                "workspace_id": workspace_record.workspace_id.clone(),
                "canonical_path": workspace_record.canonical_path.clone(),
                "client_type": client_type,
                "planner_status": "resolved",
                "reason": reason
            }),
        )
        .await?;

        if self.planner.compatibility_direct_dispatch {
            let task = ExternalQueryService::new(self.pool.clone())
                .get_task(task_id)
                .await?
                .ok_or_else(|| Error::Domain("planned task missing".to_string()))?;
            self.dispatch_task(
                task_id,
                &workspace_record.canonical_path,
                client_type,
                task.input,
                task.metadata,
                DispatchRoutingUpdate::Matched,
            )
            .await?;
        }
        Ok(())
    }

    async fn apply_planner_failed(
        &self,
        task_id: &str,
        reason: &str,
        decision: Option<PlannerDecision>,
    ) -> Result<()> {
        sqlx::query(
            r#"UPDATE tasks
               SET state = 'needs_confirmation', routing_state = 'failed', routing_reason = ?,
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE task_id = ?"#,
        )
        .bind(reason)
        .bind(task_id)
        .execute(&self.pool)
        .await?;
        self.record_task_event(
            task_id,
            "task.planning_failed",
            json!({"reason": reason, "decision": decision}),
        )
        .await?;
        Ok(())
    }

    async fn task_data(&self, task_id: &str) -> Result<Value> {
        let task = ExternalQueryService::new(self.pool.clone())
            .get_task(task_id)
            .await?
            .ok_or_else(|| Error::Domain("task missing".to_string()))?;
        Ok(json!({ "task": task }))
    }

    pub async fn sync_task_from_turn_event(&self, event: &DomainEvent) -> Result<()> {
        let Some(turn_id) = event.turn_id.as_deref() else {
            return Ok(());
        };
        let Some((task_id, current_state)) = sqlx::query_as::<_, (String, String)>(
            "SELECT task_id, state FROM tasks WHERE turn_id = ?",
        )
        .bind(turn_id)
        .fetch_optional(&self.pool)
        .await?
        else {
            return Ok(());
        };

        if is_terminal_task_state(&current_state) {
            return Ok(());
        }

        let transition = match event.event_type {
            EventType::TurnStarted => Some(("running", "task.running")),
            EventType::TurnCompleted => Some(("completed", "task.completed")),
            EventType::TurnFailed => Some(("failed", "task.failed")),
            EventType::TurnInterrupted | EventType::TurnCancelled => {
                Some(("cancelled", "task.cancelled"))
            }
            _ => None,
        };
        let Some((next_state, task_event_type)) = transition else {
            return Ok(());
        };

        sqlx::query(
            r#"UPDATE tasks
               SET state = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE task_id = ? AND turn_id = ?"#,
        )
        .bind(next_state)
        .bind(&task_id)
        .bind(turn_id)
        .execute(&self.pool)
        .await?;
        self.record_task_event(
            &task_id,
            task_event_type,
            json!({"turn_id": turn_id, "domain_event_id": event.event_id}),
        )
        .await?;
        Ok(())
    }

    async fn dispatch_task(
        &self,
        task_id: &str,
        workspace: &str,
        client_type: &str,
        input: String,
        metadata: Value,
        routing_update: DispatchRoutingUpdate,
    ) -> Result<()> {
        sqlx::query(
            r#"UPDATE tasks
               SET state = 'routing', updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE task_id = ?"#,
        )
        .bind(task_id)
        .execute(&self.pool)
        .await?;
        self.record_task_event(task_id, "task.routing_started", json!({}))
            .await?;

        let workspace_record = upsert_workspace(&self.pool, workspace).await?;
        let routing_state = match routing_update {
            DispatchRoutingUpdate::Matched => "matched",
            DispatchRoutingUpdate::Confirmed => "confirmed",
        };
        sqlx::query(
            r#"UPDATE tasks
               SET workspace_id = ?, routing_state = ?, routing_confidence = 1.0,
                   routing_reason = NULL,
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE task_id = ?"#,
        )
        .bind(&workspace_record.workspace_id)
        .bind(routing_state)
        .bind(task_id)
        .execute(&self.pool)
        .await?;
        let workspace_event = match routing_update {
            DispatchRoutingUpdate::Matched => "task.workspace_matched",
            DispatchRoutingUpdate::Confirmed => "task.workspace_confirmed",
        };
        self.record_task_event(
            task_id,
            workspace_event,
            json!({"workspace_id": workspace_record.workspace_id, "canonical_path": workspace_record.canonical_path}),
        )
        .await?;

        let session_id = self
            .find_idle_session(&workspace_record.workspace_id, client_type)
            .await?;
        let session_id = if let Some(session_id) = session_id {
            self.record_task_event(
                task_id,
                "task.session_selected",
                json!({"session_id": session_id}),
            )
            .await?;
            session_id
        } else {
            let session_outcome = SessionCommandService::new(self.pool.clone())
                .create_session(
                    CreateSessionRequest {
                        client_type: client_type.to_string(),
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
                task_id,
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
        .bind(task_id)
        .execute(&self.pool)
        .await?;

        let turn_outcome = TurnCommandService::new(self.pool.clone())
            .submit_turn(&session_id, SubmitTurnRequest { input, metadata }, None)
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
        .bind(task_id)
        .execute(&self.pool)
        .await?;
        self.record_task_event(task_id, "task.turn_created", json!({"turn_id": turn_id}))
            .await?;
        Ok(())
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

    async fn mark_task_failed(&self, task_id: &str, reason: &str) -> Result<()> {
        sqlx::query(
            r#"UPDATE tasks
               SET state = 'failed', routing_state = 'failed', routing_reason = ?,
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE task_id = ?"#,
        )
        .bind(reason)
        .bind(task_id)
        .execute(&self.pool)
        .await?;
        self.record_task_event(task_id, "task.failed", json!({"reason": reason}))
            .await?;
        Ok(())
    }

    async fn idempotency_response(&self, operation: &str, key: &str) -> Result<Option<Value>> {
        let response: Option<String> = sqlx::query_scalar(
            "SELECT response FROM idempotency_keys WHERE operation = ? AND key = ?",
        )
        .bind(operation)
        .bind(key)
        .fetch_optional(&self.pool)
        .await?;

        response
            .map(|value| serde_json::from_str(&value))
            .transpose()
            .map_err(Into::into)
    }

    async fn store_idempotency_response(
        &self,
        operation: &str,
        key: &str,
        response: &Value,
    ) -> Result<()> {
        sqlx::query(
            r#"INSERT INTO idempotency_keys (operation, key, response)
               VALUES (?, ?, ?)
               ON CONFLICT(operation, key) DO NOTHING"#,
        )
        .bind(operation)
        .bind(key)
        .bind(serde_json::to_string(response)?)
        .execute(&self.pool)
        .await?;
        Ok(())
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

#[derive(Debug, Clone, Copy)]
enum DispatchRoutingUpdate {
    Matched,
    Confirmed,
}

fn is_terminal_task_state(state: &str) -> bool {
    matches!(state, "completed" | "failed" | "cancelled")
}
