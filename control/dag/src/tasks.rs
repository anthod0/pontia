use super::*;
use pontia_application::{get_workspace_record, upsert_workspace};
use pontia_storage_sqlite::repositories::{
    dag::SqliteDagRepository,
    idempotency::SqliteIdempotencyRepository,
    tasks::{CreateTaskRecord, SqliteTaskRepository},
};

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct CreateDagTaskRequest {
    pub input: String,
    pub workspace: Option<String>,
    pub workspace_id: Option<String>,
    #[serde(default = "default_client_type")]
    pub client_type: String,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct HumanSignalRequest {
    pub kind: String,
    pub summary: String,
    pub detail: Option<String>,
    #[serde(default = "default_signal_severity")]
    pub severity: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DagTaskCommandOutcome {
    pub data: Value,
    pub duplicate: bool,
}

#[derive(Clone)]
pub struct DagTaskCommandService {
    pool: SqlitePool,
    graph: GraphRuntimeConfig,
}

impl DagTaskCommandService {
    pub fn new(pool: SqlitePool) -> Self {
        Self::with_graph(pool, GraphRuntimeConfig::default())
    }

    pub fn with_graph(pool: SqlitePool, graph: GraphRuntimeConfig) -> Self {
        Self { pool, graph }
    }

    pub async fn create_dag_task(
        &self,
        request: CreateDagTaskRequest,
        idempotency_key: Option<&str>,
    ) -> Result<DagTaskCommandOutcome> {
        let workspace = request.workspace.as_deref().unwrap_or_default().trim();
        let workspace_id = request.workspace_id.as_deref().unwrap_or_default().trim();
        if workspace.is_empty() && workspace_id.is_empty() {
            return Err(Error::Domain(
                "workspace or workspace_id is required for DAG tasks".to_string(),
            ));
        }
        if !workspace.is_empty() && !workspace_id.is_empty() {
            return Err(Error::Domain(
                "workspace and workspace_id cannot both be provided".to_string(),
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
            return Ok(DagTaskCommandOutcome {
                data: response,
                duplicate: true,
            });
        }

        let workspace_record = if !workspace_id.is_empty() {
            get_workspace_record(&self.pool, workspace_id)
                .await?
                .ok_or_else(|| Error::NotFound(format!("workspace {workspace_id} not found")))?
        } else {
            upsert_workspace(&self.pool, workspace).await?
        };
        let task_id = pontia_core::ids::new_task_id().to_string();
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

        SqliteTaskRepository::new(self.pool.clone())
            .create_task(CreateTaskRecord {
                task_id: task_id.clone(),
                state: "created".to_string(),
                input: request.input.clone(),
                workspace_id: Some(workspace_record.workspace_id.clone()),
                routing_state: "matched".to_string(),
                routing_confidence: Some(1.0),
                metadata: serde_json::to_string(&metadata)?,
            })
            .await?;
        self.record_task_event(&task_id, "task.created", json!({ "mode": "dag" }))
            .await?;
        self.record_task_event(
            &task_id,
            "task.workspace_matched",
            json!({"workspace_id": workspace_record.workspace_id, "canonical_path": workspace_record.canonical_path}),
        )
        .await?;

        let planning_turn = DagPlanningService::with_graph(self.pool.clone(), self.graph.clone())
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
        Ok(DagTaskCommandOutcome {
            data,
            duplicate: false,
        })
    }

    pub async fn pause_task(
        &self,
        task_id: &str,
        idempotency_key: Option<&str>,
    ) -> Result<DagTaskCommandOutcome> {
        if let Some(key) = idempotency_key
            && let Some(response) = self
                .idempotency_response(&format!("pause_task:{task_id}"), key)
                .await?
        {
            return Ok(DagTaskCommandOutcome {
                data: response,
                duplicate: true,
            });
        }
        let task = self.require_non_terminal_task(task_id).await?;
        SqliteTaskRepository::new(self.pool.clone())
            .update_task_state(task_id, "paused")
            .await?;
        self.record_task_event(task_id, "task.paused", json!({}))
            .await?;
        let data = json!({ "task": ExternalQueryService::new(self.pool.clone()).get_task(task_id).await?.unwrap_or(task) });
        if let Some(key) = idempotency_key {
            self.store_idempotency_response(&format!("pause_task:{task_id}"), key, &data)
                .await?;
        }
        Ok(DagTaskCommandOutcome {
            data,
            duplicate: false,
        })
    }

    pub async fn resume_task(
        &self,
        task_id: &str,
        idempotency_key: Option<&str>,
    ) -> Result<DagTaskCommandOutcome> {
        if let Some(key) = idempotency_key
            && let Some(response) = self
                .idempotency_response(&format!("resume_task:{task_id}"), key)
                .await?
        {
            return Ok(DagTaskCommandOutcome {
                data: response,
                duplicate: true,
            });
        }
        let task = self.require_non_terminal_task(task_id).await?;
        if task.state != "paused" {
            return Err(Error::StateConflict(format!(
                "task {task_id} is not paused"
            )));
        }
        SqliteTaskRepository::new(self.pool.clone())
            .update_task_state(task_id, "running")
            .await?;
        self.record_task_event(task_id, "task.resumed", json!({}))
            .await?;
        let scheduler = DagSchedulerService::with_graph(self.pool.clone(), self.graph.clone())
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
        Ok(DagTaskCommandOutcome {
            data,
            duplicate: false,
        })
    }

    pub async fn create_human_signal(
        &self,
        task_id: &str,
        request: HumanSignalRequest,
        idempotency_key: Option<&str>,
    ) -> Result<DagTaskCommandOutcome> {
        if let Some(key) = idempotency_key
            && let Some(response) = self
                .idempotency_response(&format!("human_signal:{task_id}"), key)
                .await?
        {
            return Ok(DagTaskCommandOutcome {
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
            "signal.emitted",
            json!({
                "signal_id": signal_id,
                "task_id": task_id,
                "source": "human",
                "kind": kind,
                "summary": summary,
                "detail": request.detail,
                "severity": severity,
                "related_refs": [],
                "state": "open",
            }),
        )
        .await?;
        GraphProjectionService::new(self.pool.clone(), self.graph.clone())
            .project_task(task_id)
            .await?;
        let row = SqliteDagRepository::new(self.pool.clone())
            .get_dag_signal(&signal_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("signal {signal_id} not found")))?;
        let data = json!({ "signal": dag_signal_row_to_record(row)? });
        if let Some(key) = idempotency_key {
            self.store_idempotency_response(&format!("human_signal:{task_id}"), key, &data)
                .await?;
        }
        Ok(DagTaskCommandOutcome {
            data,
            duplicate: false,
        })
    }

    async fn require_non_terminal_task(&self, task_id: &str) -> Result<TaskView> {
        let task = ExternalQueryService::new(self.pool.clone())
            .get_task(task_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("task {task_id} not found")))?;
        if matches!(task.state.as_str(), "completed" | "failed" | "cancelled") {
            return Err(Error::StateConflict(format!(
                "task {task_id} is already terminal"
            )));
        }
        Ok(task)
    }

    async fn idempotency_response(&self, operation: &str, key: &str) -> Result<Option<Value>> {
        SqliteIdempotencyRepository::new(self.pool.clone())
            .get_response(operation, key)
            .await
    }

    async fn store_idempotency_response(
        &self,
        operation: &str,
        key: &str,
        response: &Value,
    ) -> Result<()> {
        SqliteIdempotencyRepository::new(self.pool.clone())
            .store_response(operation, key, response)
            .await
    }

    async fn record_task_event(
        &self,
        task_id: &str,
        event_type: &str,
        payload: Value,
    ) -> Result<()> {
        let event_id = new_event_id().to_string();
        let payload = serde_json::to_string(&payload)?;
        SqliteTaskRepository::new(self.pool.clone())
            .record_task_event(&event_id, task_id, event_type, &payload)
            .await?;
        if self.graph.enabled
            && let Err(error) = GraphProjectionService::new(self.pool.clone(), self.graph.clone())
                .project_task(task_id)
                .await
        {
            tracing::warn!(task_id, event_type, error = %error, "graph projection failed");
        }
        Ok(())
    }
}

fn default_signal_severity() -> String {
    "medium".to_string()
}
