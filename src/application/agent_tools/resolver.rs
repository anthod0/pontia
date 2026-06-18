use super::*;
use pontia_storage_sqlite::repositories::runtime_bindings::SqliteRuntimeBindingRepository;

pub struct AgentToolContextResolver {
    pool: SqlitePool,
}

impl AgentToolContextResolver {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn resolve(&self, request: &AgentToolRequest) -> Result<AgentToolContext> {
        validate_required("session_id", &request.session_id)?;
        validate_required("turn_id", &request.turn_id)?;
        validate_required("runtime_instance_id", &request.runtime_instance_id)?;

        let session = self.load_session(&request.session_id).await?;
        if matches!(session.state.as_str(), "exited" | "error") {
            return Err(Error::StateConflict(format!(
                "session {} is terminal",
                request.session_id
            )));
        }

        let turn = self.load_turn(&request.turn_id).await?;
        if turn.session_id != request.session_id {
            return Err(Error::StateConflict(format!(
                "turn {} does not belong to session {}",
                request.turn_id, request.session_id
            )));
        }

        let runtime_instance_id = self.runtime_instance_id(&request.session_id).await?;
        if runtime_instance_id != request.runtime_instance_id {
            return Err(Error::StateConflict(format!(
                "runtime_instance_id does not match session {}",
                request.session_id
            )));
        }

        if !turn
            .metadata
            .get("dag_managed")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            return Err(Error::StateConflict(format!(
                "turn {} is not DAG-managed",
                request.turn_id
            )));
        }

        let mode = if let Some(role) = turn
            .metadata
            .get("dag_planning_role")
            .and_then(Value::as_str)
        {
            AgentToolMode::Planning {
                role: parse_planning_role(role)?,
            }
        } else {
            let run = self.execution_run_for_turn(&request.turn_id).await?;
            AgentToolMode::Execution {
                run_id: run.run_id,
                work_item_id: run.work_item_id,
            }
        };

        let task_id = match &mode {
            AgentToolMode::Planning { .. } => turn
                .metadata
                .get("task_id")
                .or_else(|| session.metadata.get("task_id"))
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .map(str::to_string)
                .ok_or_else(|| {
                    Error::StateConflict(format!(
                        "DAG-managed planning turn {} is missing task_id",
                        request.turn_id
                    ))
                })?,
            AgentToolMode::Execution { run_id, .. } => self
                .task_id_for_run(run_id)
                .await?
                .ok_or_else(|| Error::NotFound(format!("work item run {run_id} not found")))?,
        };

        Ok(AgentToolContext {
            session_id: request.session_id.clone(),
            turn_id: request.turn_id.clone(),
            client_type: session.client_type,
            runtime_instance_id,
            task_id,
            mode,
        })
    }

    async fn load_session(&self, session_id: &str) -> Result<SessionForAgentTool> {
        let row = sqlx::query(
            "SELECT session_id, client_type, state, metadata FROM sessions WHERE session_id = ?",
        )
        .bind(session_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| Error::NotFound(format!("session {session_id} not found")))?;
        let metadata: String = row.try_get("metadata")?;
        Ok(SessionForAgentTool {
            client_type: row.try_get("client_type")?,
            state: row.try_get("state")?,
            metadata: serde_json::from_str(&metadata)?,
        })
    }

    async fn load_turn(&self, turn_id: &str) -> Result<TurnForAgentTool> {
        let row =
            sqlx::query("SELECT turn_id, session_id, state, metadata FROM turns WHERE turn_id = ?")
                .bind(turn_id)
                .fetch_optional(&self.pool)
                .await?
                .ok_or_else(|| Error::NotFound(format!("turn {turn_id} not found")))?;
        let metadata: String = row.try_get("metadata")?;
        Ok(TurnForAgentTool {
            session_id: row.try_get("session_id")?,
            metadata: serde_json::from_str(&metadata)?,
        })
    }

    async fn runtime_instance_id(&self, session_id: &str) -> Result<String> {
        let metadata = SqliteRuntimeBindingRepository::new(self.pool.clone())
            .metadata(session_id)
            .await?
            .ok_or_else(|| {
                Error::StateConflict(format!("session {session_id} has no runtime binding"))
            })?;
        let metadata: Value = serde_json::from_str(&metadata)?;
        metadata
            .get("runtime_instance_id")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string)
            .ok_or_else(|| {
                Error::StateConflict(format!(
                    "session {session_id} runtime binding missing runtime_instance_id"
                ))
            })
    }

    async fn execution_run_for_turn(&self, turn_id: &str) -> Result<ExecutionRunForAgentTool> {
        let row = sqlx::query(
            r#"SELECT run_id, work_item_id
               FROM work_item_runs
               WHERE turn_id = ?
               ORDER BY created_at DESC, run_id DESC LIMIT 1"#,
        )
        .bind(turn_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| {
            Error::StateConflict(format!(
                "DAG-managed turn {turn_id} is not an execution turn"
            ))
        })?;
        Ok(ExecutionRunForAgentTool {
            run_id: row.try_get("run_id")?,
            work_item_id: row.try_get("work_item_id")?,
        })
    }

    async fn task_id_for_run(&self, run_id: &str) -> Result<Option<String>> {
        sqlx::query_scalar("SELECT task_id FROM work_item_runs WHERE run_id = ?")
            .bind(run_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(Error::from)
    }
}

struct SessionForAgentTool {
    client_type: String,
    state: String,
    metadata: Value,
}

struct TurnForAgentTool {
    session_id: String,
    metadata: Value,
}

struct ExecutionRunForAgentTool {
    run_id: String,
    work_item_id: String,
}
