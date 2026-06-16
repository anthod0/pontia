use super::*;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DagPlanningTurn {
    pub task_id: String,
    pub session_id: String,
    pub turn_id: String,
    pub profile_id: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DagPlanningOutcome {
    pub proposal: DagProposal,
    pub scheduler: DagSchedulerOutcome,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DagApplyOutcome {
    pub proposal: DagProposal,
    pub scheduler: DagSchedulerOutcome,
}

#[derive(Clone)]
pub struct DagPlanningService {
    pool: SqlitePool,
    graph: GraphRuntimeConfig,
}

impl DagPlanningService {
    pub fn new(pool: SqlitePool) -> Self {
        Self::with_graph(pool, GraphRuntimeConfig::default())
    }

    pub fn with_graph(pool: SqlitePool, graph: GraphRuntimeConfig) -> Self {
        Self { pool, graph }
    }

    pub async fn start_initial_planning(&self, task_id: &str) -> Result<DagPlanningTurn> {
        self.start_initial_planning_for_client_type(task_id, None)
            .await
    }

    pub async fn start_initial_planning_with_client_type(
        &self,
        task_id: &str,
        client_type: &str,
    ) -> Result<DagPlanningTurn> {
        self.start_initial_planning_for_client_type(task_id, Some(client_type))
            .await
    }

    async fn start_initial_planning_for_client_type(
        &self,
        task_id: &str,
        client_type: Option<&str>,
    ) -> Result<DagPlanningTurn> {
        let task = self.task_context(task_id).await?;
        sqlx::query(
            r#"UPDATE tasks
               SET state = 'planning', updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE task_id = ?"#,
        )
        .bind(task_id)
        .execute(&self.pool)
        .await?;
        self.record_task_event(
            task_id,
            "task.planning_started",
            json!({"mode":"initial_dag"}),
        )
        .await?;
        self.create_planning_turn(
            task_id,
            "planner",
            task.input,
            json!({"mode":"initial_dag"}),
            client_type,
            task.workspace_id.as_deref(),
        )
        .await
    }

    pub async fn submit_planner_output(
        &self,
        task_id: &str,
        session_id: &str,
        raw_output: String,
    ) -> Result<DagPlanningOutcome> {
        let payload = parse_initial_plan_output(&raw_output)?;
        let turn_id = self.latest_planning_turn_id(task_id, session_id).await?;
        self.submit_initial_plan_payload(task_id, session_id, &turn_id, payload)
            .await
    }

    pub async fn submit_initial_plan_payload(
        &self,
        task_id: &str,
        session_id: &str,
        turn_id: &str,
        payload: SubmitPlanPayload,
    ) -> Result<DagPlanningOutcome> {
        dag_validator::validate_plan_shape(&payload)?;
        let dag = DagService::with_graph(self.pool.clone(), self.graph.clone());
        let proposal = dag
            .save_proposal(task_id, &payload, Some(session_id), turn_id)
            .await?;
        sqlx::query(
            r#"UPDATE tasks
               SET state = 'awaiting_approval', updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE task_id = ?"#,
        )
        .bind(task_id)
        .execute(&self.pool)
        .await?;
        self.record_task_event(
            task_id,
            "dag.proposed",
            json!({"proposal_id": proposal.proposal_id, "mode": proposal.mode, "revision": proposal.revision}),
        )
        .await?;
        Ok(DagPlanningOutcome {
            proposal,
            scheduler: DagSchedulerOutcome {
                dispatched_runs: Vec::new(),
            },
        })
    }

    pub async fn start_replanning_for_signal(
        &self,
        task_id: &str,
        signal_id: &str,
    ) -> Result<DagPlanningTurn> {
        let task = self.task_context(task_id).await?;
        let signal = sqlx::query(
            r#"SELECT signal_id, kind, summary, detail, severity
               FROM dag_signals WHERE task_id = ? AND signal_id = ?"#,
        )
        .bind(task_id)
        .bind(signal_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| Error::NotFound(format!("signal {signal_id} not found")))?;
        sqlx::query(
            r#"UPDATE tasks
               SET state = 'replanning', updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE task_id = ?"#,
        )
        .bind(task_id)
        .execute(&self.pool)
        .await?;
        self.record_task_event(
            task_id,
            "task.replanning_started",
            json!({"mode":"patch", "signal_id": signal_id}),
        )
        .await?;
        let prompt = format!(
            "Replan task {task_id} by producing a DAG patch.\n\nTask input:\n{}\n\nSignal {} [{} {}]: {}\n{}",
            task.input,
            signal.get::<String, _>("signal_id"),
            signal.get::<String, _>("kind"),
            signal.get::<String, _>("severity"),
            signal.get::<String, _>("summary"),
            signal
                .try_get::<Option<String>, _>("detail")?
                .unwrap_or_default()
        );
        self.create_planning_turn(
            task_id,
            "replanner",
            prompt,
            json!({"mode":"patch", "signal_id": signal_id}),
            None,
            task.workspace_id.as_deref(),
        )
        .await
    }

    pub async fn submit_replanner_output(
        &self,
        task_id: &str,
        session_id: &str,
        raw_output: String,
    ) -> Result<DagPlanningOutcome> {
        let (summary, patch) = parse_patch_output(&raw_output)?;
        let turn_id = self.latest_planning_turn_id(task_id, session_id).await?;
        self.submit_patch_payload(task_id, session_id, &turn_id, summary, patch)
            .await
    }

    pub async fn submit_patch_payload(
        &self,
        task_id: &str,
        session_id: &str,
        turn_id: &str,
        summary: String,
        patch: DagPatch,
    ) -> Result<DagPlanningOutcome> {
        let dag = DagService::with_graph(self.pool.clone(), self.graph.clone());
        let proposal = dag
            .save_patch_proposal(task_id, &summary, &patch, Some(session_id), turn_id)
            .await?;
        sqlx::query(
            r#"UPDATE tasks
               SET state = 'awaiting_approval', updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE task_id = ?"#,
        )
        .bind(task_id)
        .execute(&self.pool)
        .await?;
        self.record_task_event(
            task_id,
            "dag.patch_proposed",
            json!({"proposal_id": proposal.proposal_id, "revision": proposal.revision}),
        )
        .await?;
        Ok(DagPlanningOutcome {
            proposal,
            scheduler: DagSchedulerOutcome {
                dispatched_runs: Vec::new(),
            },
        })
    }

    pub async fn apply_proposal(
        &self,
        task_id: &str,
        session_id: &str,
        proposal_id: &str,
        approval_quote: Option<String>,
        approval_message_ref: Option<String>,
    ) -> Result<DagApplyOutcome> {
        let dag = DagService::with_graph(self.pool.clone(), self.graph.clone());
        let proposal = dag.get_proposal(proposal_id).await?;
        if proposal.task_id != task_id {
            return Err(Error::StateConflict(format!(
                "proposal {proposal_id} belongs to task {}, not {task_id}",
                proposal.task_id
            )));
        }
        if proposal.state != "proposed" {
            return Err(Error::StateConflict(format!(
                "proposal {proposal_id} is not proposed (state {})",
                proposal.state
            )));
        }

        self.record_task_event(
            task_id,
            "human.approved",
            json!({
                "proposal_id": proposal.proposal_id,
                "approval_quote": approval_quote,
                "approval_message_ref": approval_message_ref,
                "approved_by_session_id": session_id,
            }),
        )
        .await?;

        let apply_result = if proposal.mode == "initial_dag" {
            let payload: SubmitPlanPayload =
                serde_json::from_value(proposal.proposal_json.clone())?;
            dag.apply_initial_dag(task_id, &payload).await?;
            json!({ "ok": true, "mode": "initial_dag" })
        } else if proposal.mode == "patch" {
            let patch_value = proposal
                .proposal_json
                .get("patch")
                .cloned()
                .ok_or_else(|| Error::Domain(format!("proposal {proposal_id} missing patch")))?;
            let patch: DagPatch = serde_json::from_value(patch_value)?;
            let summary = dag.apply_patch(task_id, &patch).await?;
            if let Some(signal_id) = self.planning_signal_id(session_id).await? {
                sqlx::query(
                    r#"UPDATE dag_signals
                       SET state = 'acknowledged', updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                       WHERE task_id = ? AND signal_id = ? AND state = 'open'"#,
                )
                .bind(task_id)
                .bind(signal_id)
                .execute(&self.pool)
                .await?;
            }
            serde_json::to_value(&summary)?
        } else {
            return Err(Error::Domain(format!(
                "proposal {proposal_id} has unsupported mode {}",
                proposal.mode
            )));
        };

        let proposal = dag
            .mark_proposal_applied_with_result(&proposal.proposal_id, apply_result)
            .await?;
        sqlx::query(
            r#"UPDATE tasks
               SET state = 'running', updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE task_id = ?"#,
        )
        .bind(task_id)
        .execute(&self.pool)
        .await?;
        self.record_task_event(
            task_id,
            "dag.approved",
            json!({"proposal_id": proposal.proposal_id, "mode": proposal.mode, "revision": proposal.revision}),
        )
        .await?;
        let scheduler = DagSchedulerService::with_graph(self.pool.clone(), self.graph.clone())
            .schedule_task(task_id)
            .await?;
        Box::pin(RuntimeControlService::new(self.pool.clone()).terminate_session(session_id, None))
            .await?;
        Ok(DagApplyOutcome {
            proposal,
            scheduler,
        })
    }

    async fn create_planning_turn(
        &self,
        task_id: &str,
        profile_id: &str,
        prompt: String,
        metadata: Value,
        preferred_client_type: Option<&str>,
        workspace_id: Option<&str>,
    ) -> Result<DagPlanningTurn> {
        let profile = AgentProfileService::new(self.pool.clone())
            .get_latest(profile_id)
            .await?
            .ok_or_else(|| {
                Error::Domain(format!("execution profile {profile_id} does not exist"))
            })?;
        if profile.agent_kind != "planner" {
            return Err(Error::Domain(format!(
                "planning profile {profile_id}@{} requires agent_kind planner, got {}",
                profile.version, profile.agent_kind
            )));
        }
        let client_type = if let Some(client_type) = preferred_client_type {
            if !profile
                .supported_client_types
                .iter()
                .any(|value| value == client_type)
            {
                return Err(Error::Domain(format!(
                    "execution profile {profile_id} does not support client_type {client_type}"
                )));
            }
            client_type.to_string()
        } else {
            profile
                .supported_client_types
                .first()
                .cloned()
                .unwrap_or_else(default_client_type)
        };
        let session = SessionCommandService::new(self.pool.clone())
            .create_session(
                CreateSessionRequest {
                    client_type,
                    title: Some(format!("Plan task {task_id}")),
                    workspace: None,
                    workspace_id: workspace_id.map(str::to_string),
                    handle: None,
                    role: profile.default_session_role.clone(),
                    description: profile.default_session_description.clone(),
                    execution_profile_id: Some(profile.profile_id.clone()),
                    execution_profile_version: Some(profile.version.clone()),
                    metadata: json!({
                        "dag_managed": true,
                        "dag_planning_role": profile_id,
                        "task_id": task_id,
                        "planning": metadata,
                    }),
                    initial_task: None,
                },
                None,
            )
            .await?;
        let session_id = session
            .data
            .pointer("/session/session_id")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| {
                Error::Domain("created planning session response missing session_id".to_string())
            })?;
        let turn = TurnCommandService::new(self.pool.clone())
            .create_and_dispatch_turn(
                &session_id,
                prompt,
                json!({
                    "task_id": task_id,
                    "dag_managed": true,
                    "dag_planning_role": profile_id,
                    "planning": metadata,
                }),
            )
            .await?;
        let turn = turn.ok_or_else(|| {
            Error::Domain("dag planning dispatch requires an immediate backend turn".to_string())
        })?;
        Ok(DagPlanningTurn {
            task_id: task_id.to_string(),
            session_id,
            turn_id: turn.turn_id,
            profile_id: profile_id.to_string(),
        })
    }

    async fn latest_planning_turn_id(&self, task_id: &str, session_id: &str) -> Result<String> {
        sqlx::query_scalar(
            r#"SELECT turn_id
               FROM turns
               WHERE session_id = ?
                 AND json_extract(metadata, '$.dag_managed') = 1
                 AND json_extract(metadata, '$.dag_planning_role') IS NOT NULL
                 AND json_extract(metadata, '$.task_id') = ?
               ORDER BY created_at DESC, turn_id DESC
               LIMIT 1"#,
        )
        .bind(session_id)
        .bind(task_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| {
            Error::StateConflict(format!(
                "planning session {session_id} has no DAG planning turn for task {task_id}"
            ))
        })
    }

    async fn planning_signal_id(&self, session_id: &str) -> Result<Option<String>> {
        let metadata: Option<String> =
            sqlx::query_scalar("SELECT metadata FROM sessions WHERE session_id = ?")
                .bind(session_id)
                .fetch_optional(&self.pool)
                .await?;
        let Some(metadata) = metadata else {
            return Ok(None);
        };
        let value: Value = serde_json::from_str(&metadata)?;
        Ok(value
            .pointer("/planning/signal_id")
            .and_then(Value::as_str)
            .map(str::to_string))
    }

    async fn task_context(&self, task_id: &str) -> Result<PlanningTaskContext> {
        let row = sqlx::query("SELECT task_id, input, workspace_id FROM tasks WHERE task_id = ?")
            .bind(task_id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| Error::NotFound(format!("task {task_id}")))?;
        Ok(PlanningTaskContext {
            input: row.get("input"),
            workspace_id: row.try_get("workspace_id")?,
        })
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

struct PlanningTaskContext {
    input: String,
    workspace_id: Option<String>,
}

fn parse_initial_plan_output(raw_output: &str) -> Result<SubmitPlanPayload> {
    let value: Value = serde_json::from_str(raw_output)?;
    let mode = value
        .get("mode")
        .and_then(Value::as_str)
        .unwrap_or("initial_dag");
    if mode != "initial_dag" {
        return Err(Error::Domain(format!(
            "planner output mode must be initial_dag, got {mode}"
        )));
    }
    let dag = value.get("dag").cloned().unwrap_or_else(|| value.clone());
    Ok(SubmitPlanPayload {
        mode: "initial_dag".to_string(),
        summary: required_string(&value, "summary")?,
        work_items: serde_json::from_value(
            dag.get("work_items").cloned().unwrap_or_else(|| json!([])),
        )?,
        edges: serde_json::from_value(dag.get("edges").cloned().unwrap_or_else(|| json!([])))?,
        assumptions: serde_json::from_value(
            value
                .get("assumptions")
                .cloned()
                .unwrap_or_else(|| json!([])),
        )?,
        risks: serde_json::from_value(value.get("risks").cloned().unwrap_or_else(|| json!([])))?,
    })
}

fn parse_patch_output(raw_output: &str) -> Result<(String, DagPatch)> {
    let value: Value = serde_json::from_str(raw_output)?;
    let mode = value.get("mode").and_then(Value::as_str).unwrap_or("patch");
    if mode != "patch" {
        return Err(Error::Domain(format!(
            "replanner output mode must be patch, got {mode}"
        )));
    }
    let summary = required_string(&value, "summary")?;
    let mut patch_value = value.get("patch").cloned().unwrap_or_else(
        || json!({"operations": value.get("operations").cloned().unwrap_or_else(|| json!([]))}),
    );
    if patch_value.get("summary").is_none()
        && let Some(object) = patch_value.as_object_mut()
    {
        object.insert("summary".to_string(), Value::String(summary.clone()));
    }
    let mut patch: DagPatch = serde_json::from_value(patch_value)?;
    if patch.summary.is_empty() {
        patch.summary = summary.clone();
    }
    Ok((summary, patch))
}

fn required_string(value: &Value, key: &str) -> Result<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| Error::Domain(format!("planner output missing string field {key}")))
}
