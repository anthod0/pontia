use std::path::PathBuf;

use serde_json::Value;
use sqlx::{Row, SqlitePool};

use crate::error::{Error, Result};

use super::lbug_backend::project_snapshot_to_lbug;
#[cfg(feature = "lbug")]
use super::lbug_backend::query_task_provenance;
use super::snapshot::{GraphDecision, GraphEvidence, TaskGraphSnapshot, snapshot_to_provenance};
use super::{GraphRuntimeConfig, TaskProvenance};

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
        if !self.config.enabled {
            return Ok(());
        }
        let Some(db_dir) = self.config.db_dir.clone() else {
            return Err(Error::InvalidConfig {
                key: "LLMPARTY_GRAPH_DB_DIR",
                message: "graph projection is enabled but no database directory is configured"
                    .to_string(),
            });
        };

        let snapshot = self.load_task_snapshot(task_id).await?;
        project_snapshot_to_lbug(PathBuf::from(db_dir), snapshot)
    }

    pub async fn task_provenance(&self, task_id: &str) -> Result<TaskProvenance> {
        if !self.config.enabled {
            return Ok(TaskProvenance {
                nodes: vec![],
                edges: vec![],
            });
        }

        #[cfg(feature = "lbug")]
        if let Some(db_dir) = self.config.db_dir.clone() {
            return query_task_provenance(PathBuf::from(db_dir), task_id);
        }

        let snapshot = self.load_task_snapshot(task_id).await?;
        Ok(snapshot_to_provenance(snapshot))
    }

    async fn load_task_snapshot(&self, task_id: &str) -> Result<TaskGraphSnapshot> {
        let task_row = sqlx::query(
            r#"SELECT task_id, input, created_at, updated_at
               FROM tasks WHERE task_id = ?"#,
        )
        .bind(task_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| Error::NotFound(format!("task {task_id} not found")))?;

        let event_rows = sqlx::query(
            r#"SELECT event_type, payload, created_at
               FROM task_events WHERE task_id = ? ORDER BY created_at, event_id"#,
        )
        .bind(task_id)
        .fetch_all(&self.pool)
        .await?;

        let mut decisions = Vec::new();
        for row in event_rows {
            let event_type: String = row.try_get("event_type")?;
            if !matches!(
                event_type.as_str(),
                "task.planning_completed"
                    | "task.planning_resolved"
                    | "task.planning_needs_input"
                    | "task.planning_failed"
            ) {
                continue;
            }
            let payload: String = row.try_get("payload")?;
            let payload: Value = serde_json::from_str(&payload)?;
            let Some(decision_value) = payload.get("decision").filter(|value| value.is_object())
            else {
                continue;
            };
            let decision_id = decision_value
                .get("decision_id")
                .and_then(Value::as_str)
                .unwrap_or("dec_unknown")
                .to_string();
            if decisions
                .iter()
                .any(|decision: &GraphDecision| decision.decision_id == decision_id)
            {
                continue;
            }
            let workspace_confidence = decision_value
                .get("workspace")
                .and_then(|workspace| workspace.get("confidence"))
                .and_then(Value::as_f64)
                .unwrap_or(0.0)
                .clamp(0.0, 1.0);
            let mut evidence = Vec::new();
            if let Some(items) = decision_value.get("evidence").and_then(Value::as_array) {
                for (index, item) in items.iter().enumerate() {
                    evidence.push(GraphEvidence {
                        evidence_id: item
                            .get("evidence_id")
                            .and_then(Value::as_str)
                            .map(ToString::to_string)
                            .unwrap_or_else(|| format!("{decision_id}_ev_{index}")),
                        kind: item
                            .get("kind")
                            .and_then(Value::as_str)
                            .unwrap_or("other")
                            .to_string(),
                        reference: item
                            .get("ref")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string(),
                        summary: item
                            .get("summary")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string(),
                    });
                }
            }
            decisions.push(GraphDecision {
                decision_id,
                status: decision_value
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
                    .to_string(),
                reason: decision_value
                    .get("reason")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                confidence: workspace_confidence,
                created_at: row.try_get("created_at")?,
                evidence,
            });
        }

        Ok(TaskGraphSnapshot {
            task_id: task_row.try_get("task_id")?,
            task_input: task_row.try_get("input")?,
            task_created_at: task_row.try_get("created_at")?,
            task_updated_at: task_row.try_get("updated_at")?,
            decisions,
        })
    }
}
