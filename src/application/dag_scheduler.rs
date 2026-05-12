use super::*;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DagSchedulerOutcome {
    pub dispatched_runs: Vec<DagSchedulerDispatch>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DagSchedulerDispatch {
    pub work_item_id: String,
    pub run_id: String,
    pub session_id: String,
    pub turn_id: String,
}

#[derive(Debug, Clone)]
pub(crate) struct SchedulerTaskContext {
    pub task_id: String,
    pub input: String,
    pub workspace_id: Option<String>,
    pub preferred_client_type: Option<String>,
    pub paused: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct SchedulerWorkItem {
    pub work_item_id: String,
    pub task_id: String,
    pub title: String,
    pub description: String,
    pub kind: String,
    pub action: String,
    pub execution_profile_id: String,
    pub execution_profile_version: Option<String>,
    pub current_attempt: i64,
}

#[derive(Clone)]
pub struct DagSchedulerService {
    pool: SqlitePool,
}

impl DagSchedulerService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn schedule_task(&self, task_id: &str) -> Result<DagSchedulerOutcome> {
        let task = self.task_context(task_id).await?;
        if task.paused {
            return Ok(DagSchedulerOutcome {
                dispatched_runs: Vec::new(),
            });
        }
        self.recompute_ready(task_id).await?;

        let mut dispatched_runs = Vec::new();
        while let Some(work_item) = self.claim_ready_work_item(task_id).await? {
            let profile = self.resolve_execution_profile(&work_item).await?;
            let run_id = self.create_work_item_run(&work_item, &profile).await?;
            let session_id = self
                .find_or_create_session(&task, &work_item, &profile)
                .await?;
            let prompt = prompt_rendering::render_work_item_prompt(
                profile.turn_prompt_template.as_deref(),
                &task,
                &work_item,
                &run_id,
            );
            let client_type =
                preferred_client_type(&profile, task.preferred_client_type.as_deref());
            let turn_id = self
                .dispatch_run(&run_id, &work_item, &session_id, &client_type, prompt)
                .await?;
            dispatched_runs.push(DagSchedulerDispatch {
                work_item_id: work_item.work_item_id,
                run_id,
                session_id,
                turn_id,
            });
        }

        Ok(DagSchedulerOutcome { dispatched_runs })
    }

    pub async fn recompute_ready(&self, task_id: &str) -> Result<()> {
        ensure_task_not_terminal(&self.pool, task_id).await?;
        sqlx::query(
            r#"UPDATE work_item_runtime_projection
               SET current_state = CASE
                       WHEN EXISTS (
                           SELECT 1 FROM work_item_edges e
                           JOIN work_items upstream ON upstream.work_item_id = e.from_work_item_id
                           LEFT JOIN work_item_runtime_projection up ON up.work_item_id = upstream.work_item_id
                           WHERE e.task_id = work_item_runtime_projection.task_id
                             AND e.to_work_item_id = work_item_runtime_projection.work_item_id
                             AND e.edge_type = 'depends_on'
                             AND upstream.active = 1
                             AND COALESCE(up.current_state, 'pending') != 'completed'
                       ) THEN 'blocked'
                       ELSE 'ready'
                   END,
                   ready_at = CASE
                       WHEN EXISTS (
                           SELECT 1 FROM work_item_edges e
                           JOIN work_items upstream ON upstream.work_item_id = e.from_work_item_id
                           LEFT JOIN work_item_runtime_projection up ON up.work_item_id = upstream.work_item_id
                           WHERE e.task_id = work_item_runtime_projection.task_id
                             AND e.to_work_item_id = work_item_runtime_projection.work_item_id
                             AND e.edge_type = 'depends_on'
                             AND upstream.active = 1
                             AND COALESCE(up.current_state, 'pending') != 'completed'
                       ) THEN NULL
                       ELSE COALESCE(ready_at, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                   END,
                   blocked_reason = CASE
                       WHEN EXISTS (
                           SELECT 1 FROM work_item_edges e
                           JOIN work_items upstream ON upstream.work_item_id = e.from_work_item_id
                           LEFT JOIN work_item_runtime_projection up ON up.work_item_id = upstream.work_item_id
                           WHERE e.task_id = work_item_runtime_projection.task_id
                             AND e.to_work_item_id = work_item_runtime_projection.work_item_id
                             AND e.edge_type = 'depends_on'
                             AND upstream.active = 1
                             AND COALESCE(up.current_state, 'pending') != 'completed'
                       ) THEN 'waiting_for_dependencies'
                       ELSE NULL
                   END,
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE task_id = ?
                 AND current_state IN ('pending', 'ready', 'blocked')
                 AND NOT EXISTS (
                     SELECT 1 FROM work_item_runs r
                     WHERE r.run_id = work_item_runtime_projection.current_run_id
                       AND r.state IN ('queued', 'running')
                 )"#,
        )
        .bind(task_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub(crate) async fn claim_ready_work_item(
        &self,
        task_id: &str,
    ) -> Result<Option<SchedulerWorkItem>> {
        ensure_task_not_terminal(&self.pool, task_id).await?;
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query(
            r#"SELECT wi.work_item_id, wi.task_id, wi.title, wi.description, wi.kind, wi.action,
                      wi.execution_profile_id, wi.execution_profile_version, p.current_attempt
               FROM work_items wi
               JOIN work_item_runtime_projection p ON p.work_item_id = wi.work_item_id
               WHERE wi.task_id = ?
                 AND wi.active = 1
                 AND p.current_state = 'ready'
                 AND NOT EXISTS (
                     SELECT 1 FROM work_item_runs r
                     WHERE r.work_item_id = wi.work_item_id AND r.state IN ('queued', 'running')
                 )
                 AND NOT EXISTS (
                     SELECT 1 FROM work_item_edges e
                     JOIN work_items upstream ON upstream.work_item_id = e.from_work_item_id
                     LEFT JOIN work_item_runtime_projection up ON up.work_item_id = upstream.work_item_id
                     WHERE e.task_id = wi.task_id
                       AND e.to_work_item_id = wi.work_item_id
                       AND e.edge_type = 'depends_on'
                       AND upstream.active = 1
                       AND COALESCE(up.current_state, 'pending') != 'completed'
                 )
                 AND EXISTS (
                     SELECT 1 FROM execution_profiles ep
                     WHERE ep.profile_id = wi.execution_profile_id
                       AND (wi.execution_profile_version IS NULL OR ep.version = wi.execution_profile_version)
                 )
               ORDER BY p.priority DESC, p.ready_at, wi.work_item_id
               LIMIT 1"#,
        )
        .bind(task_id)
        .fetch_optional(&mut *tx)
        .await?;

        let Some(row) = row else {
            tx.commit().await?;
            return Ok(None);
        };
        let work_item_id: String = row.get("work_item_id");
        let updated = sqlx::query(
            r#"UPDATE work_item_runtime_projection
               SET current_state = 'running', updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE task_id = ? AND work_item_id = ? AND current_state = 'ready'"#,
        )
        .bind(task_id)
        .bind(&work_item_id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if updated == 0 {
            tx.commit().await?;
            return Ok(None);
        }
        tx.commit().await?;

        Ok(Some(SchedulerWorkItem {
            work_item_id,
            task_id: row.get("task_id"),
            title: row.get("title"),
            description: row.get("description"),
            kind: row.get("kind"),
            action: row.get("action"),
            execution_profile_id: row.get("execution_profile_id"),
            execution_profile_version: row.get("execution_profile_version"),
            current_attempt: row.get("current_attempt"),
        }))
    }

    async fn create_work_item_run(
        &self,
        work_item: &SchedulerWorkItem,
        profile: &ExecutionProfileView,
    ) -> Result<String> {
        let run_id = new_scheduler_id("wirun");
        let attempt = work_item.current_attempt + 1;
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            r#"INSERT INTO work_item_runs (
                    run_id, work_item_id, task_id, attempt, state,
                    execution_profile_id, execution_profile_version, rendered_prompt_ref
               ) VALUES (?, ?, ?, ?, 'running', ?, ?, 'inline')"#,
        )
        .bind(&run_id)
        .bind(&work_item.work_item_id)
        .bind(&work_item.task_id)
        .bind(attempt)
        .bind(&profile.profile_id)
        .bind(&profile.version)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            r#"UPDATE work_item_runtime_projection
               SET current_run_id = ?, current_state = 'running', current_attempt = ?,
                   blocked_reason = NULL, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE task_id = ? AND work_item_id = ? AND current_state = 'running' AND current_run_id IS NULL"#,
        )
        .bind(&run_id)
        .bind(attempt)
        .bind(&work_item.task_id)
        .bind(&work_item.work_item_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(run_id)
    }

    async fn resolve_execution_profile(
        &self,
        work_item: &SchedulerWorkItem,
    ) -> Result<ExecutionProfileView> {
        if let Some(version) = &work_item.execution_profile_version {
            AgentProfileService::new(self.pool.clone())
                .get_version(&work_item.execution_profile_id, version)
                .await?
                .ok_or_else(|| {
                    Error::Domain(format!(
                        "execution profile {} version {} does not exist",
                        work_item.execution_profile_id, version
                    ))
                })
        } else {
            AgentProfileService::new(self.pool.clone())
                .get_latest(&work_item.execution_profile_id)
                .await?
                .ok_or_else(|| {
                    Error::Domain(format!(
                        "execution profile {} does not exist",
                        work_item.execution_profile_id
                    ))
                })
        }
    }

    async fn find_or_create_session(
        &self,
        task: &SchedulerTaskContext,
        work_item: &SchedulerWorkItem,
        profile: &ExecutionProfileView,
    ) -> Result<String> {
        if profile.session_reuse_policy != "fresh_per_run"
            && let Some(session_id) = self
                .find_idle_reusable_session(task, work_item, profile)
                .await?
        {
            return Ok(session_id);
        }

        let client_type = preferred_client_type(profile, task.preferred_client_type.as_deref());
        let outcome = SessionCommandService::new(self.pool.clone())
            .create_session(
                CreateSessionRequest {
                    client_type,
                    workspace: None,
                    workspace_id: task.workspace_id.clone(),
                    handle: None,
                    role: profile.default_session_role.clone(),
                    description: profile.default_session_description.clone(),
                    execution_profile_id: Some(profile.profile_id.clone()),
                    execution_profile_version: Some(profile.version.clone()),
                    metadata: json!({
                        "dag_managed": true,
                        "task_id": task.task_id,
                        "work_item_id": work_item.work_item_id,
                    }),
                    initial_task: None,
                },
                None,
            )
            .await?;
        outcome
            .data
            .pointer("/session/session_id")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| Error::Domain("created session response missing session_id".to_string()))
    }

    async fn find_idle_reusable_session(
        &self,
        task: &SchedulerTaskContext,
        work_item: &SchedulerWorkItem,
        profile: &ExecutionProfileView,
    ) -> Result<Option<String>> {
        let row = match profile.session_reuse_policy.as_str() {
            "reuse_by_workspace_and_profile" => {
                let Some(workspace_id) = task.workspace_id.as_deref() else {
                    return Ok(None);
                };
                sqlx::query_scalar(
                    r#"SELECT session_id FROM sessions
                       WHERE state = 'idle' AND current_turn_id IS NULL
                         AND workspace_id = ?
                         AND execution_profile_id = ? AND execution_profile_version = ?
                       ORDER BY updated_at DESC, session_id LIMIT 1"#,
                )
                .bind(workspace_id)
                .bind(&profile.profile_id)
                .bind(&profile.version)
                .fetch_optional(&self.pool)
                .await?
            }
            "reuse_by_task_and_profile" => {
                sqlx::query_scalar(
                    r#"SELECT session_id FROM sessions
                       WHERE state = 'idle' AND current_turn_id IS NULL
                         AND execution_profile_id = ? AND execution_profile_version = ?
                         AND json_extract(metadata, '$.task_id') = ?
                       ORDER BY updated_at DESC, session_id LIMIT 1"#,
                )
                .bind(&profile.profile_id)
                .bind(&profile.version)
                .bind(&task.task_id)
                .fetch_optional(&self.pool)
                .await?
            }
            "fresh_per_work_item" => {
                sqlx::query_scalar(
                    r#"SELECT session_id FROM sessions
                       WHERE state = 'idle' AND current_turn_id IS NULL
                         AND execution_profile_id = ? AND execution_profile_version = ?
                         AND json_extract(metadata, '$.work_item_id') = ?
                       ORDER BY updated_at DESC, session_id LIMIT 1"#,
                )
                .bind(&profile.profile_id)
                .bind(&profile.version)
                .bind(&work_item.work_item_id)
                .fetch_optional(&self.pool)
                .await?
            }
            _ => None,
        };
        Ok(row)
    }

    async fn dispatch_run(
        &self,
        run_id: &str,
        work_item: &SchedulerWorkItem,
        session_id: &str,
        client_type: &str,
        prompt: String,
    ) -> Result<String> {
        let turn = TurnCommandService::new(self.pool.clone())
            .create_and_dispatch_turn(
                session_id,
                prompt.clone(),
                json!({
                    "task_id": work_item.task_id,
                    "work_item_id": work_item.work_item_id,
                    "run_id": run_id,
                    "dag_managed": true,
                }),
            )
            .await?;
        sqlx::query("UPDATE turns SET input_summary = ? WHERE turn_id = ?")
            .bind(&prompt)
            .bind(&turn.turn_id)
            .execute(&self.pool)
            .await?;
        sqlx::query(
            r#"UPDATE work_item_runs
               SET session_id = ?, turn_id = ?, client_type = ?, started_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE run_id = ?"#,
        )
        .bind(session_id)
        .bind(&turn.turn_id)
        .bind(client_type)
        .bind(run_id)
        .execute(&self.pool)
        .await?;
        sqlx::query(
            r#"UPDATE work_item_runtime_projection
               SET session_id = ?, turn_id = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE work_item_id = ? AND current_run_id = ?"#,
        )
        .bind(session_id)
        .bind(&turn.turn_id)
        .bind(&work_item.work_item_id)
        .bind(run_id)
        .execute(&self.pool)
        .await?;
        Ok(turn.turn_id)
    }

    async fn task_context(&self, task_id: &str) -> Result<SchedulerTaskContext> {
        let row = sqlx::query(
            "SELECT task_id, state, input, workspace_id, metadata FROM tasks WHERE task_id = ?",
        )
        .bind(task_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| Error::NotFound(format!("task {task_id}")))?;
        let metadata: String = row.get("metadata");
        let preferred_client_type =
            serde_json::from_str::<Value>(&metadata)
                .ok()
                .and_then(|value| {
                    value
                        .get("planner_client_type")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                });
        Ok(SchedulerTaskContext {
            task_id: row.get("task_id"),
            input: row.get("input"),
            workspace_id: row.get("workspace_id"),
            preferred_client_type,
            paused: row.get::<String, _>("state") == "paused",
        })
    }
}

async fn ensure_task_not_terminal(pool: &SqlitePool, task_id: &str) -> Result<()> {
    let state: Option<String> = sqlx::query_scalar("SELECT state FROM tasks WHERE task_id = ?")
        .bind(task_id)
        .fetch_optional(pool)
        .await?;
    match state.as_deref() {
        None => Err(Error::NotFound(format!("task {task_id}"))),
        Some("completed" | "failed" | "cancelled") => {
            Err(Error::StateConflict(format!("task {task_id} is terminal")))
        }
        Some(_) => Ok(()),
    }
}

fn preferred_client_type(profile: &ExecutionProfileView, task_preferred: Option<&str>) -> String {
    if let Some(client_type) = task_preferred
        && profile
            .supported_client_types
            .iter()
            .any(|value| value == client_type)
    {
        return client_type.to_string();
    }
    if profile
        .supported_client_types
        .iter()
        .any(|value| value == "generic")
    {
        "generic".to_string()
    } else {
        profile
            .supported_client_types
            .first()
            .cloned()
            .unwrap_or_else(default_client_type)
    }
}

fn new_scheduler_id(prefix: &str) -> String {
    format!("{prefix}_{}", uuid::Uuid::now_v7())
}
