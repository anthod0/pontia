use serde_json::{Value, json};
use sqlx::{Row, SqlitePool};

use pontia_core::error::{Error, Result};

#[cfg(feature = "lbug")]
use super::LbugDagGraphStore;
use super::{
    AddWorkItemEdgeRequest, GraphEdgeKind, GraphRuntimeConfig, TaskGraphSnapshot, TaskProvenance,
    UpsertSignalRequest, UpsertTaskRequest, UpsertWorkItemRequest, WorkItemNode,
};

#[derive(Clone)]
pub struct GraphProjectionService {
    pool: SqlitePool,
    config: GraphRuntimeConfig,
}

impl GraphProjectionService {
    pub fn new(pool: SqlitePool, config: GraphRuntimeConfig) -> Self {
        Self { pool, config }
    }

    pub async fn project_task(&self, task_id: &str) -> Result<()> {
        let rows = sqlx::query(
            r#"SELECT event_id, event_type, payload, created_at
               FROM task_events
               WHERE task_id = ?
               ORDER BY rowid"#,
        )
        .bind(task_id)
        .fetch_all(&self.pool)
        .await?;

        for row in rows {
            let event_id: String = row.get("event_id");
            let event_type: String = row.get("event_type");
            let created_at: String = row.get("created_at");
            let payload: String = row.get("payload");
            let payload: Value = serde_json::from_str(&payload)?;
            self.project_event(task_id, &event_id, &event_type, &payload, &created_at)
                .await?;
        }
        Ok(())
    }

    pub async fn task_provenance(&self, _task_id: &str) -> Result<TaskProvenance> {
        Ok(TaskProvenance {
            nodes: vec![],
            edges: vec![],
        })
    }

    pub async fn task_graph(&self, task_id: &str) -> Result<TaskGraphSnapshot> {
        self.lbug_store().await?.task_graph(task_id).await
    }

    pub async fn get_work_item(&self, work_item_id: &str) -> Result<Option<WorkItemNode>> {
        self.lbug_store().await?.get_work_item(work_item_id).await
    }

    async fn project_event(
        &self,
        task_id: &str,
        event_id: &str,
        event_type: &str,
        payload: &Value,
        created_at: &str,
    ) -> Result<()> {
        let event_ref = Some(format!("event:{event_type}:{event_id}"));
        match event_type {
            "task.created" => {
                self.upsert_task(UpsertTaskRequest {
                    task_id: task_id.to_string(),
                    title: string(payload, "title")
                        .or_else(|| string(payload, "input"))
                        .unwrap_or_else(|| task_id.to_string()),
                    description: string(payload, "description")
                        .or_else(|| string(payload, "input"))
                        .unwrap_or_default(),
                    ref_: event_ref,
                    metadata: payload.clone(),
                })
                .await?;
            }
            "dag.applied" | "dag.patch_applied" => {
                self.ensure_task_node(task_id, payload, event_ref).await?;
            }
            "work_item.created" => {
                self.ensure_task_node(task_id, &json!({}), None).await?;
                let work_item = payload.get("work_item").unwrap_or(payload);
                self.upsert_work_item(UpsertWorkItemRequest {
                    work_item_id: string(work_item, "work_item_id").unwrap_or_default(),
                    task_id: string(work_item, "task_id").unwrap_or_else(|| task_id.to_string()),
                    title: string(work_item, "title").unwrap_or_default(),
                    description: string(work_item, "description").unwrap_or_default(),
                    kind: string(work_item, "kind").unwrap_or_else(|| "other".to_string()),
                    action: string(work_item, "action").unwrap_or_else(|| "agent_turn".to_string()),
                    execution_profile_id: string(work_item, "execution_profile_id")
                        .unwrap_or_else(|| "default".to_string()),
                    execution_profile_version: string(work_item, "execution_profile_version"),
                    review_policy: work_item.get("review_policy").cloned(),
                    execution_policy: work_item.get("execution_policy").cloned(),
                    escalation_policy: work_item.get("escalation_policy").cloned(),
                    priority: work_item
                        .get("priority")
                        .and_then(Value::as_i64)
                        .unwrap_or_default(),
                    optional: work_item
                        .get("optional")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                    parallelizable: work_item
                        .get("parallelizable")
                        .and_then(Value::as_bool)
                        .unwrap_or(true),
                    acceptance_criteria: work_item
                        .get("acceptance_criteria")
                        .cloned()
                        .unwrap_or_else(|| json!([])),
                    active: work_item
                        .get("active")
                        .and_then(Value::as_bool)
                        .unwrap_or(true),
                    ref_: event_ref,
                    metadata: work_item
                        .get("metadata")
                        .cloned()
                        .unwrap_or_else(|| json!({})),
                })
                .await?;
            }
            "work_item.edge_added" => {
                self.add_edge(AddWorkItemEdgeRequest {
                    task_id: string(payload, "task_id").unwrap_or_else(|| task_id.to_string()),
                    from_work_item_id: string(payload, "from_work_item_id").unwrap_or_default(),
                    to_work_item_id: string(payload, "to_work_item_id").unwrap_or_default(),
                    edge_type: string(payload, "edge_type")
                        .as_deref()
                        .and_then(GraphEdgeKind::parse)
                        .unwrap_or(GraphEdgeKind::DependsOn),
                    ref_: event_ref,
                })
                .await?;
            }
            "work_item.edge_removed" => {
                self.remove_edge(
                    &string(payload, "task_id").unwrap_or_else(|| task_id.to_string()),
                    &string(payload, "from_work_item_id").unwrap_or_default(),
                    &string(payload, "to_work_item_id").unwrap_or_default(),
                    string(payload, "edge_type")
                        .as_deref()
                        .and_then(GraphEdgeKind::parse)
                        .unwrap_or(GraphEdgeKind::DependsOn),
                )
                .await?;
            }
            "work_item.reactivated" => {
                if let Some(work_item_id) = string(payload, "work_item_id") {
                    self.set_work_item_active(&work_item_id, true).await?;
                }
            }
            "work_item.superseded" => {
                if let Some(work_item_id) = string(payload, "work_item_id") {
                    self.set_work_item_active(&work_item_id, false).await?;
                    if let Some(replacement_id) = string(payload, "replacement_work_item_id")
                        .or_else(|| string(payload, "replacement_id"))
                    {
                        self.add_edge(AddWorkItemEdgeRequest {
                            task_id: task_id.to_string(),
                            from_work_item_id: replacement_id,
                            to_work_item_id: work_item_id,
                            edge_type: GraphEdgeKind::Supersedes,
                            ref_: event_ref,
                        })
                        .await?;
                    }
                }
            }
            "signal.emitted" => {
                self.ensure_task_node(task_id, &json!({}), None).await?;
                self.upsert_signal(UpsertSignalRequest {
                    signal_id: string(payload, "signal_id").unwrap_or_default(),
                    task_id: string(payload, "task_id").unwrap_or_else(|| task_id.to_string()),
                    work_item_id: string(payload, "work_item_id"),
                    run_id: string(payload, "run_id"),
                    source_session_id: string(payload, "source_session_id"),
                    source: string(payload, "source").unwrap_or_else(|| "system".to_string()),
                    kind: string(payload, "kind").unwrap_or_else(|| "other".to_string()),
                    summary: string(payload, "summary").unwrap_or_default(),
                    detail: string(payload, "detail"),
                    severity: string(payload, "severity").unwrap_or_else(|| "medium".to_string()),
                    related_refs: payload
                        .get("related_refs")
                        .cloned()
                        .unwrap_or_else(|| json!([])),
                    state: string(payload, "state").unwrap_or_else(|| "open".to_string()),
                    ref_: event_ref,
                    metadata: payload
                        .get("metadata")
                        .cloned()
                        .unwrap_or_else(|| json!({})),
                })
                .await?;
            }
            _ => {}
        }
        let _ = created_at;
        Ok(())
    }

    async fn ensure_task_node(
        &self,
        task_id: &str,
        payload: &Value,
        ref_: Option<String>,
    ) -> Result<()> {
        if self.task_graph(task_id).await?.task.is_some() {
            return Ok(());
        }
        let input: Option<String> = sqlx::query_scalar("SELECT input FROM tasks WHERE task_id = ?")
            .bind(task_id)
            .fetch_optional(&self.pool)
            .await?;
        self.upsert_task(UpsertTaskRequest {
            task_id: task_id.to_string(),
            title: string(payload, "title")
                .or_else(|| input.clone())
                .unwrap_or_else(|| task_id.to_string()),
            description: string(payload, "description").or(input).unwrap_or_default(),
            ref_,
            metadata: json!({}),
        })
        .await
    }

    async fn upsert_task(&self, request: UpsertTaskRequest) -> Result<()> {
        self.lbug_store().await?.upsert_task(request).await
    }

    async fn upsert_work_item(&self, request: UpsertWorkItemRequest) -> Result<()> {
        self.lbug_store().await?.upsert_work_item(request).await
    }

    async fn set_work_item_active(&self, work_item_id: &str, active: bool) -> Result<()> {
        self.lbug_store()
            .await?
            .set_work_item_active(work_item_id, active)
            .await
    }

    async fn add_edge(&self, request: AddWorkItemEdgeRequest) -> Result<()> {
        self.lbug_store().await?.add_edge(request).await
    }

    async fn remove_edge(
        &self,
        task_id: &str,
        from_work_item_id: &str,
        to_work_item_id: &str,
        edge_type: GraphEdgeKind,
    ) -> Result<()> {
        self.lbug_store()
            .await?
            .remove_edge(task_id, from_work_item_id, to_work_item_id, edge_type)
            .await
    }

    async fn upsert_signal(&self, request: UpsertSignalRequest) -> Result<()> {
        self.lbug_store().await?.upsert_signal(request).await
    }

    #[cfg(feature = "lbug")]
    async fn lbug_store(&self) -> Result<LbugDagGraphStore> {
        if !self.config.enabled {
            return Err(lbug_required_error());
        }
        let db_dir = self
            .config
            .db_dir
            .as_ref()
            .ok_or_else(|| Error::InvalidConfig {
                key: "PONTIA_HOME",
                message: "graph.enabled requires a Ladybug database path derived from pontia home"
                    .to_string(),
            })?;
        LbugDagGraphStore::open(db_dir).await
    }

    #[cfg(not(feature = "lbug"))]
    async fn lbug_store(&self) -> Result<LbugDagGraphStoreUnavailable> {
        let _ = &self.config;
        Err(lbug_required_error())
    }
}

#[cfg(not(feature = "lbug"))]
struct LbugDagGraphStoreUnavailable;

#[cfg(not(feature = "lbug"))]
impl LbugDagGraphStoreUnavailable {
    async fn task_graph(&self, _task_id: &str) -> Result<TaskGraphSnapshot> {
        Err(lbug_required_error())
    }

    async fn get_work_item(&self, _work_item_id: &str) -> Result<Option<WorkItemNode>> {
        Err(lbug_required_error())
    }

    async fn upsert_task(&self, _request: UpsertTaskRequest) -> Result<()> {
        Err(lbug_required_error())
    }

    async fn upsert_work_item(&self, _request: UpsertWorkItemRequest) -> Result<()> {
        Err(lbug_required_error())
    }

    async fn set_work_item_active(&self, _work_item_id: &str, _active: bool) -> Result<()> {
        Err(lbug_required_error())
    }

    async fn add_edge(&self, _request: AddWorkItemEdgeRequest) -> Result<()> {
        Err(lbug_required_error())
    }

    async fn remove_edge(
        &self,
        _task_id: &str,
        _from_work_item_id: &str,
        _to_work_item_id: &str,
        _edge_type: GraphEdgeKind,
    ) -> Result<()> {
        Err(lbug_required_error())
    }

    async fn upsert_signal(&self, _request: UpsertSignalRequest) -> Result<()> {
        Err(lbug_required_error())
    }
}

fn lbug_required_error() -> Error {
    Error::CapabilityUnavailable(
        "WorkItem DAG graph storage requires enabling graph.enabled with an pontia build that includes the `lbug` feature".to_string(),
    )
}

fn string(value: &Value, key: &str) -> Option<String> {
    value.get(key)?.as_str().map(ToString::to_string)
}
