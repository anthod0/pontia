use std::collections::{HashMap, HashSet};

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
        if task.paused || self.has_open_blocking_signal(task_id).await? {
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
        let snapshot = SqliteDagGraphStore::new(self.pool.clone())
            .task_graph(task_id)
            .await?;
        let active_ids: HashSet<String> = snapshot
            .work_items
            .iter()
            .filter(|work_item| work_item.active)
            .map(|work_item| work_item.work_item_id.clone())
            .collect();
        let state_rows = sqlx::query(
            "SELECT work_item_id, current_state FROM work_item_runtime_projection WHERE task_id = ?",
        )
        .bind(task_id)
        .fetch_all(&self.pool)
        .await?;
        let states: HashMap<String, String> = state_rows
            .into_iter()
            .map(|row| (row.get("work_item_id"), row.get("current_state")))
            .collect();
        let rows = sqlx::query(
            r#"SELECT work_item_id, current_state
               FROM work_item_runtime_projection
               WHERE task_id = ?
                 AND current_state IN ('pending', 'ready', 'blocked')
                 AND NOT EXISTS (
                     SELECT 1 FROM work_item_runs r
                     WHERE r.run_id = work_item_runtime_projection.current_run_id
                       AND r.state IN ('queued', 'running')
                 )"#,
        )
        .bind(task_id)
        .fetch_all(&self.pool)
        .await?;
        for row in rows {
            let work_item_id: String = row.get("work_item_id");
            if !active_ids.contains(&work_item_id) {
                continue;
            }
            let has_blocking_dependency = snapshot.edges.iter().any(|edge| {
                edge.edge_type == GraphEdgeKind::DependsOn
                    && edge.to_work_item_id == work_item_id
                    && active_ids.contains(&edge.from_work_item_id)
                    && !matches!(
                        states.get(&edge.from_work_item_id).map(String::as_str),
                        Some("completed") | Some("replan_anchor")
                    )
            });
            let state = if has_blocking_dependency {
                "blocked"
            } else {
                "ready"
            };
            sqlx::query(
                r#"UPDATE work_item_runtime_projection
                   SET current_state = ?,
                       ready_at = CASE WHEN ? = 'ready' THEN COALESCE(ready_at, strftime('%Y-%m-%dT%H:%M:%fZ', 'now')) ELSE NULL END,
                       blocked_reason = CASE WHEN ? = 'blocked' THEN 'waiting_for_dependencies' ELSE NULL END,
                       updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                   WHERE task_id = ? AND work_item_id = ?"#,
            )
            .bind(state)
            .bind(state)
            .bind(state)
            .bind(task_id)
            .bind(&work_item_id)
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }

    pub(crate) async fn claim_ready_work_item(
        &self,
        task_id: &str,
    ) -> Result<Option<SchedulerWorkItem>> {
        ensure_task_not_terminal(&self.pool, task_id).await?;
        let snapshot = SqliteDagGraphStore::new(self.pool.clone())
            .task_graph(task_id)
            .await?;
        let active_items: HashMap<String, WorkItemNode> = snapshot
            .work_items
            .iter()
            .filter(|work_item| work_item.active)
            .map(|work_item| (work_item.work_item_id.clone(), work_item.clone()))
            .collect();
        let active_ids: HashSet<String> = active_items.keys().cloned().collect();
        let state_rows = sqlx::query(
            "SELECT work_item_id, current_state FROM work_item_runtime_projection WHERE task_id = ?",
        )
        .bind(task_id)
        .fetch_all(&self.pool)
        .await?;
        let states: HashMap<String, String> = state_rows
            .into_iter()
            .map(|row| (row.get("work_item_id"), row.get("current_state")))
            .collect();
        let rows = sqlx::query(
            r#"SELECT work_item_id, current_attempt
               FROM work_item_runtime_projection
               WHERE task_id = ? AND current_state = 'ready'
                 AND NOT EXISTS (
                     SELECT 1 FROM work_item_runs r
                     WHERE r.work_item_id = work_item_runtime_projection.work_item_id
                       AND r.state IN ('queued', 'running')
                 )
               ORDER BY priority DESC, ready_at, work_item_id"#,
        )
        .bind(task_id)
        .fetch_all(&self.pool)
        .await?;

        for row in rows {
            let work_item_id: String = row.get("work_item_id");
            let Some(work_item) = active_items.get(&work_item_id) else {
                sqlx::query(
                    r#"UPDATE work_item_runtime_projection
                       SET current_state = 'blocked', blocked_reason = 'missing_or_inactive_graph_work_item',
                           ready_at = NULL, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                       WHERE task_id = ? AND work_item_id = ?"#,
                )
                .bind(task_id)
                .bind(&work_item_id)
                .execute(&self.pool)
                .await?;
                continue;
            };
            let dependencies_blocking = snapshot.edges.iter().any(|edge| {
                edge.edge_type == GraphEdgeKind::DependsOn
                    && edge.to_work_item_id == work_item_id
                    && active_ids.contains(&edge.from_work_item_id)
                    && !matches!(
                        states.get(&edge.from_work_item_id).map(String::as_str),
                        Some("completed") | Some("replan_anchor")
                    )
            });
            if dependencies_blocking {
                sqlx::query(
                    r#"UPDATE work_item_runtime_projection
                       SET current_state = 'blocked', blocked_reason = 'waiting_for_dependencies',
                           ready_at = NULL, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                       WHERE task_id = ? AND work_item_id = ? AND current_state = 'ready'"#,
                )
                .bind(task_id)
                .bind(&work_item_id)
                .execute(&self.pool)
                .await?;
                continue;
            }
            if !profile_exists(&self.pool, work_item).await? {
                sqlx::query(
                    r#"UPDATE work_item_runtime_projection
                       SET current_state = 'blocked', blocked_reason = 'missing_execution_profile',
                           ready_at = NULL, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                       WHERE task_id = ? AND work_item_id = ? AND current_state = 'ready'"#,
                )
                .bind(task_id)
                .bind(&work_item_id)
                .execute(&self.pool)
                .await?;
                continue;
            }

            let mut tx = self.pool.begin().await?;
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
            tx.commit().await?;
            if updated == 0 {
                return Ok(None);
            }

            return Ok(Some(SchedulerWorkItem {
                work_item_id,
                task_id: work_item.task_id.clone(),
                title: work_item.title.clone(),
                description: work_item.description.clone(),
                kind: work_item.kind.clone(),
                action: work_item.action.clone(),
                execution_profile_id: work_item.execution_profile_id.clone(),
                execution_profile_version: work_item.execution_profile_version.clone(),
                current_attempt: row.get("current_attempt"),
            }));
        }

        Ok(None)
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

    async fn has_open_blocking_signal(&self, task_id: &str) -> Result<bool> {
        let count: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM dag_signals
               WHERE task_id = ?
                 AND state = 'open'
                 AND kind IN ('replan_requested', 'needs_input', 'missing_dependency', 'scope_change')"#,
        )
        .bind(task_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(count > 0)
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
            paused: matches!(
                row.get::<String, _>("state").as_str(),
                "paused" | "replanning"
            ),
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

async fn profile_exists(pool: &SqlitePool, work_item: &WorkItemNode) -> Result<bool> {
    let exists: Option<i64> = if let Some(version) = &work_item.execution_profile_version {
        sqlx::query_scalar("SELECT 1 FROM execution_profiles WHERE profile_id = ? AND version = ?")
            .bind(&work_item.execution_profile_id)
            .bind(version)
            .fetch_optional(pool)
            .await?
    } else {
        sqlx::query_scalar("SELECT 1 FROM execution_profiles WHERE profile_id = ? LIMIT 1")
            .bind(&work_item.execution_profile_id)
            .fetch_optional(pool)
            .await?
    };
    Ok(exists.is_some())
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
    profile
        .supported_client_types
        .first()
        .cloned()
        .unwrap_or_else(default_client_type)
}

fn new_scheduler_id(prefix: &str) -> String {
    format!("{prefix}_{}", uuid::Uuid::now_v7())
}
