use super::*;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentToolRequest {
    pub session_id: String,
    pub turn_id: String,
    pub runtime_instance_id: String,
    #[serde(default = "default_agent_tool_input")]
    pub input: Value,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct AgentToolResponse {
    pub context: AgentToolContext,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct AgentToolContext {
    pub session_id: String,
    pub turn_id: String,
    pub client_type: String,
    pub runtime_instance_id: String,
    pub task_id: String,
    pub mode: AgentToolMode,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentToolMode {
    Planning {
        role: AgentPlanningRole,
    },
    Execution {
        run_id: String,
        work_item_id: String,
    },
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentPlanningRole {
    Planner,
    Replanner,
}

#[derive(Clone)]
pub struct AgentToolService {
    resolver: AgentToolContextResolver,
}

impl AgentToolService {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            resolver: AgentToolContextResolver::new(pool),
        }
    }

    pub async fn call(
        &self,
        tool_name: &str,
        request: AgentToolRequest,
    ) -> Result<AgentToolResponse> {
        if !is_known_tool(tool_name) {
            return Err(Error::NotFound(format!("agent tool {tool_name} not found")));
        }
        let context = self.resolver.resolve(&request).await?;
        Ok(AgentToolResponse { context })
    }
}

#[derive(Clone)]
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
        let metadata: String =
            sqlx::query_scalar("SELECT metadata FROM runtime_bindings WHERE session_id = ?")
                .bind(session_id)
                .fetch_optional(&self.pool)
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

fn default_agent_tool_input() -> Value {
    json!({})
}

fn is_known_tool(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "getContext" | "submitPlan" | "submitResult" | "raiseSignal"
    )
}

fn validate_required(field: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        Err(Error::Domain(format!("{field} is required")))
    } else {
        Ok(())
    }
}

fn parse_planning_role(role: &str) -> Result<AgentPlanningRole> {
    match role {
        "planner" => Ok(AgentPlanningRole::Planner),
        "replanner" => Ok(AgentPlanningRole::Replanner),
        other => Err(Error::StateConflict(format!(
            "unsupported DAG planning role {other}"
        ))),
    }
}
