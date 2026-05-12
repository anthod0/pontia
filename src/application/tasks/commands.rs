use super::*;

impl TaskCommandService {
    pub async fn create_dag_task(
        &self,
        request: CreateDagTaskRequest,
        idempotency_key: Option<&str>,
    ) -> Result<CreateTaskOutcome> {
        let workspace = request.workspace.as_deref().unwrap_or_default().trim();
        if workspace.is_empty() {
            return Err(Error::Domain(
                "workspace is required for DAG tasks".to_string(),
            ));
        }
        if !is_supported_client_type(&request.client_type) {
            return Err(Error::Domain(format!(
                "unsupported client_type: {}",
                request.client_type
            )));
        }

        if let Some(key) = idempotency_key
            && let Some(response) = self.idempotency_response("create_dag_task", key).await?
        {
            return Ok(CreateTaskOutcome {
                data: response,
                duplicate: true,
            });
        }

        let workspace_record = upsert_workspace(&self.pool, workspace).await?;
        let task_id = new_task_id().to_string();
        let mut metadata = request.metadata;
        if let Some(object) = metadata.as_object_mut() {
            object.insert("dag_managed".to_string(), Value::Bool(true));
            object.insert("mode".to_string(), Value::String("dag".to_string()));
            object.insert(
                "planner_client_type".to_string(),
                Value::String(request.client_type.clone()),
            );
        } else {
            metadata = json!({
                "dag_managed": true,
                "mode": "dag",
                "planner_client_type": request.client_type.clone(),
                "original_metadata": metadata,
            });
        }

        sqlx::query(
            r#"INSERT INTO tasks (task_id, state, input, workspace_id, routing_state, routing_confidence, metadata)
               VALUES (?, 'created', ?, ?, 'matched', 1.0, ?)"#,
        )
        .bind(&task_id)
        .bind(&request.input)
        .bind(&workspace_record.workspace_id)
        .bind(serde_json::to_string(&metadata)?)
        .execute(&self.pool)
        .await?;
        self.record_task_event(&task_id, "task.created", json!({ "mode": "dag" }))
            .await?;
        self.record_task_event(
            &task_id,
            "task.workspace_matched",
            json!({"workspace_id": workspace_record.workspace_id, "canonical_path": workspace_record.canonical_path}),
        )
        .await?;

        let planning_turn = DagPlanningService::new(self.pool.clone())
            .start_initial_planning_with_client_type(&task_id, &request.client_type)
            .await?;
        let task = ExternalQueryService::new(self.pool.clone())
            .get_task(&task_id)
            .await?
            .ok_or_else(|| Error::Domain("created DAG task missing".to_string()))?;
        let data = json!({ "task": task, "planning_turn": planning_turn });
        if let Some(key) = idempotency_key {
            self.store_idempotency_response("create_dag_task", key, &data)
                .await?;
        }
        Ok(CreateTaskOutcome {
            data,
            duplicate: false,
        })
    }

    pub async fn create_task(
        &self,
        request: CreateTaskRequest,
        idempotency_key: Option<&str>,
    ) -> Result<CreateTaskOutcome> {
        if !is_supported_client_type(&request.client_type) {
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

        if !is_supported_client_type(&request.client_type) {
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

        if !is_supported_client_type(&request.client_type) {
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

    pub async fn pause_task(
        &self,
        task_id: &str,
        idempotency_key: Option<&str>,
    ) -> Result<CreateTaskOutcome> {
        if let Some(key) = idempotency_key
            && let Some(response) = self
                .idempotency_response(&format!("pause_task:{task_id}"), key)
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

        sqlx::query(
            r#"UPDATE tasks
               SET state = 'paused', updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE task_id = ?"#,
        )
        .bind(task_id)
        .execute(&self.pool)
        .await?;
        self.record_task_event(task_id, "task.paused", json!({}))
            .await?;
        let task = ExternalQueryService::new(self.pool.clone())
            .get_task(task_id)
            .await?
            .ok_or_else(|| Error::Domain("paused task missing".to_string()))?;
        let data = json!({ "task": task });
        if let Some(key) = idempotency_key {
            self.store_idempotency_response(&format!("pause_task:{task_id}"), key, &data)
                .await?;
        }
        Ok(CreateTaskOutcome {
            data,
            duplicate: false,
        })
    }

    pub async fn resume_task(
        &self,
        task_id: &str,
        idempotency_key: Option<&str>,
    ) -> Result<CreateTaskOutcome> {
        if let Some(key) = idempotency_key
            && let Some(response) = self
                .idempotency_response(&format!("resume_task:{task_id}"), key)
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
        if task.state != "paused" {
            return Err(Error::StateConflict(format!(
                "task {task_id} is not paused"
            )));
        }

        sqlx::query(
            r#"UPDATE tasks
               SET state = 'running', updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE task_id = ?"#,
        )
        .bind(task_id)
        .execute(&self.pool)
        .await?;
        self.record_task_event(task_id, "task.resumed", json!({}))
            .await?;
        let scheduler = DagSchedulerService::new(self.pool.clone())
            .schedule_task(task_id)
            .await?;
        let task = ExternalQueryService::new(self.pool.clone())
            .get_task(task_id)
            .await?
            .ok_or_else(|| Error::Domain("resumed task missing".to_string()))?;
        let data = json!({ "task": task, "scheduler": scheduler });
        if let Some(key) = idempotency_key {
            self.store_idempotency_response(&format!("resume_task:{task_id}"), key, &data)
                .await?;
        }
        Ok(CreateTaskOutcome {
            data,
            duplicate: false,
        })
    }

    pub async fn create_human_signal(
        &self,
        task_id: &str,
        request: HumanSignalRequest,
        idempotency_key: Option<&str>,
    ) -> Result<CreateTaskOutcome> {
        if let Some(key) = idempotency_key
            && let Some(response) = self
                .idempotency_response(&format!("human_signal:{task_id}"), key)
                .await?
        {
            return Ok(CreateTaskOutcome {
                data: response,
                duplicate: true,
            });
        }

        ExternalQueryService::new(self.pool.clone())
            .get_task(task_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("task {task_id} not found")))?;
        let kind = request.kind.trim();
        let summary = request.summary.trim();
        if kind.is_empty() {
            return Err(Error::Domain("signal kind is required".to_string()));
        }
        if summary.is_empty() {
            return Err(Error::Domain("signal summary is required".to_string()));
        }
        let severity = match request.severity.as_str() {
            "low" | "medium" | "high" => request.severity.as_str(),
            _ => "medium",
        };
        let signal_id = format!("dagsig_{}", uuid::Uuid::now_v7());
        sqlx::query(
            r#"INSERT INTO dag_signals (
                    signal_id, task_id, source, kind, summary, detail, severity, related_refs
               ) VALUES (?, ?, 'human', ?, ?, ?, ?, '[]')"#,
        )
        .bind(&signal_id)
        .bind(task_id)
        .bind(kind)
        .bind(summary)
        .bind(&request.detail)
        .bind(severity)
        .execute(&self.pool)
        .await?;
        self.record_task_event(
            task_id,
            "dag.signal_created",
            json!({"signal_id": signal_id, "source": "human", "kind": kind}),
        )
        .await?;
        let row = sqlx::query(
            r#"SELECT signal_id, task_id, work_item_id, run_id, source_session_id, source, kind,
                      summary, detail, severity, related_refs, state, created_at, updated_at
               FROM dag_signals WHERE signal_id = ?"#,
        )
        .bind(&signal_id)
        .fetch_one(&self.pool)
        .await?;
        let signal = row_to_dag_signal_record(row)?;
        let data = json!({ "signal": signal });
        if let Some(key) = idempotency_key {
            self.store_idempotency_response(&format!("human_signal:{task_id}"), key, &data)
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
}
