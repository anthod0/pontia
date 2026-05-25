use serde_json::Value;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::error::{Error, Result};

use super::{
    AddWorkItemEdgeRequest, GraphEdgeKind, SignalNode, TaskGraphSnapshot, TaskNode,
    UpsertSignalRequest, UpsertTaskRequest, UpsertWorkItemRequest, WorkItemEdgeRecord,
    WorkItemNode,
};

#[derive(Debug, Clone)]
pub struct SqliteDagGraphStore {
    pool: SqlitePool,
}

impl SqliteDagGraphStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn upsert_task(&self, request: UpsertTaskRequest) -> Result<()> {
        let metadata = serde_json::to_string(&request.metadata)?;
        sqlx::query(
            r#"INSERT INTO graph_tasks (task_id, title, description, ref, metadata)
               VALUES (?, ?, ?, ?, ?)
               ON CONFLICT(task_id) DO UPDATE SET
                   title = excluded.title,
                   description = excluded.description,
                   ref = excluded.ref,
                   metadata = excluded.metadata,
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')"#,
        )
        .bind(request.task_id)
        .bind(request.title)
        .bind(request.description)
        .bind(request.ref_)
        .bind(metadata)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn upsert_work_item(&self, request: UpsertWorkItemRequest) -> Result<()> {
        let review_policy = optional_json_to_string(&request.review_policy)?;
        let execution_policy = optional_json_to_string(&request.execution_policy)?;
        let escalation_policy = optional_json_to_string(&request.escalation_policy)?;
        let acceptance_criteria = serde_json::to_string(&request.acceptance_criteria)?;
        let metadata = serde_json::to_string(&request.metadata)?;
        sqlx::query(
            r#"INSERT INTO graph_work_items (
                    work_item_id, task_id, title, description, kind, action,
                    execution_profile_id, execution_profile_version, review_policy,
                    execution_policy, escalation_policy, priority, optional,
                    parallelizable, acceptance_criteria, active, ref, metadata
               ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
               ON CONFLICT(work_item_id) DO UPDATE SET
                   task_id = excluded.task_id,
                   title = excluded.title,
                   description = excluded.description,
                   kind = excluded.kind,
                   action = excluded.action,
                   execution_profile_id = excluded.execution_profile_id,
                   execution_profile_version = excluded.execution_profile_version,
                   review_policy = excluded.review_policy,
                   execution_policy = excluded.execution_policy,
                   escalation_policy = excluded.escalation_policy,
                   priority = excluded.priority,
                   optional = excluded.optional,
                   parallelizable = excluded.parallelizable,
                   acceptance_criteria = excluded.acceptance_criteria,
                   active = excluded.active,
                   ref = excluded.ref,
                   metadata = excluded.metadata,
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')"#,
        )
        .bind(request.work_item_id)
        .bind(request.task_id)
        .bind(request.title)
        .bind(request.description)
        .bind(request.kind)
        .bind(request.action)
        .bind(request.execution_profile_id)
        .bind(request.execution_profile_version)
        .bind(review_policy)
        .bind(execution_policy)
        .bind(escalation_policy)
        .bind(request.priority)
        .bind(request.optional)
        .bind(request.parallelizable)
        .bind(acceptance_criteria)
        .bind(request.active)
        .bind(request.ref_)
        .bind(metadata)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn set_work_item_active(&self, work_item_id: &str, active: bool) -> Result<()> {
        sqlx::query(
            r#"UPDATE graph_work_items
               SET active = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE work_item_id = ?"#,
        )
        .bind(active)
        .bind(work_item_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn add_edge(&self, request: AddWorkItemEdgeRequest) -> Result<()> {
        let edge_id = format!("gie_{}", Uuid::now_v7());
        sqlx::query(
            r#"INSERT INTO graph_work_item_edges (
                    edge_id, task_id, from_work_item_id, to_work_item_id, edge_type, ref, metadata
               ) VALUES (?, ?, ?, ?, ?, ?, '{}')
               ON CONFLICT(task_id, from_work_item_id, to_work_item_id, edge_type) DO UPDATE SET
                   active = 1,
                   ref = excluded.ref"#,
        )
        .bind(edge_id)
        .bind(request.task_id)
        .bind(request.from_work_item_id)
        .bind(request.to_work_item_id)
        .bind(request.edge_type.as_str())
        .bind(request.ref_)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn remove_edge(
        &self,
        task_id: &str,
        from_work_item_id: &str,
        to_work_item_id: &str,
        edge_type: GraphEdgeKind,
    ) -> Result<()> {
        sqlx::query(
            r#"UPDATE graph_work_item_edges
               SET active = 0
               WHERE task_id = ? AND from_work_item_id = ? AND to_work_item_id = ? AND edge_type = ?"#,
        )
        .bind(task_id)
        .bind(from_work_item_id)
        .bind(to_work_item_id)
        .bind(edge_type.as_str())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn upsert_signal(&self, request: UpsertSignalRequest) -> Result<()> {
        let related_refs = serde_json::to_string(&request.related_refs)?;
        let metadata = serde_json::to_string(&request.metadata)?;
        sqlx::query(
            r#"INSERT INTO graph_signals (
                    signal_id, task_id, work_item_id, run_id, source_session_id,
                    source, kind, summary, detail, severity, related_refs, state, ref, metadata
               ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
               ON CONFLICT(signal_id) DO UPDATE SET
                   task_id = excluded.task_id,
                   work_item_id = excluded.work_item_id,
                   run_id = excluded.run_id,
                   source_session_id = excluded.source_session_id,
                   source = excluded.source,
                   kind = excluded.kind,
                   summary = excluded.summary,
                   detail = excluded.detail,
                   severity = excluded.severity,
                   related_refs = excluded.related_refs,
                   state = excluded.state,
                   ref = excluded.ref,
                   metadata = excluded.metadata,
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')"#,
        )
        .bind(request.signal_id)
        .bind(request.task_id)
        .bind(request.work_item_id)
        .bind(request.run_id)
        .bind(request.source_session_id)
        .bind(request.source)
        .bind(request.kind)
        .bind(request.summary)
        .bind(request.detail)
        .bind(request.severity)
        .bind(related_refs)
        .bind(request.state)
        .bind(request.ref_)
        .bind(metadata)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn task_graph(&self, task_id: &str) -> Result<TaskGraphSnapshot> {
        let task = sqlx::query(
            r#"SELECT task_id, title, description, ref, metadata, created_at, updated_at
               FROM graph_tasks WHERE task_id = ?"#,
        )
        .bind(task_id)
        .fetch_optional(&self.pool)
        .await?
        .map(row_to_task)
        .transpose()?;

        let work_item_rows = sqlx::query(
            r#"SELECT work_item_id, task_id, title, description, kind, action,
                      execution_profile_id, execution_profile_version, review_policy,
                      execution_policy, escalation_policy, priority, optional,
                      parallelizable, acceptance_criteria, active, ref, metadata,
                      created_at, updated_at
               FROM graph_work_items
               WHERE task_id = ?
               ORDER BY priority DESC, created_at, work_item_id"#,
        )
        .bind(task_id)
        .fetch_all(&self.pool)
        .await?;
        let work_items = work_item_rows
            .into_iter()
            .map(row_to_work_item)
            .collect::<Result<Vec<_>>>()?;

        let edge_rows = sqlx::query(
            r#"SELECT edge_id, task_id, from_work_item_id, to_work_item_id, edge_type,
                      ref, metadata, created_at
               FROM graph_work_item_edges
               WHERE task_id = ? AND active = 1
               ORDER BY created_at, edge_id"#,
        )
        .bind(task_id)
        .fetch_all(&self.pool)
        .await?;
        let edges = edge_rows
            .into_iter()
            .map(row_to_edge)
            .collect::<Result<Vec<_>>>()?;

        let signal_rows = sqlx::query(
            r#"SELECT signal_id, task_id, work_item_id, run_id, source_session_id,
                      source, kind, summary, detail, severity, related_refs, state,
                      ref, metadata, created_at, updated_at
               FROM graph_signals
               WHERE task_id = ?
               ORDER BY created_at, signal_id"#,
        )
        .bind(task_id)
        .fetch_all(&self.pool)
        .await?;
        let signals = signal_rows
            .into_iter()
            .map(row_to_signal)
            .collect::<Result<Vec<_>>>()?;

        Ok(TaskGraphSnapshot {
            task,
            work_items,
            edges,
            signals,
        })
    }

    pub async fn get_work_item(&self, work_item_id: &str) -> Result<Option<WorkItemNode>> {
        sqlx::query(
            r#"SELECT work_item_id, task_id, title, description, kind, action,
                      execution_profile_id, execution_profile_version, review_policy,
                      execution_policy, escalation_policy, priority, optional,
                      parallelizable, acceptance_criteria, active, ref, metadata,
                      created_at, updated_at
               FROM graph_work_items
               WHERE work_item_id = ?"#,
        )
        .bind(work_item_id)
        .fetch_optional(&self.pool)
        .await?
        .map(row_to_work_item)
        .transpose()
    }

    pub async fn list_dependencies(&self, work_item_id: &str) -> Result<Vec<WorkItemNode>> {
        let rows = sqlx::query(
            r#"SELECT wi.work_item_id, wi.task_id, wi.title, wi.description, wi.kind, wi.action,
                      wi.execution_profile_id, wi.execution_profile_version, wi.review_policy,
                      wi.execution_policy, wi.escalation_policy, wi.priority, wi.optional,
                      wi.parallelizable, wi.acceptance_criteria, wi.active, wi.ref, wi.metadata,
                      wi.created_at, wi.updated_at
               FROM graph_work_item_edges edge
               JOIN graph_work_items wi ON wi.work_item_id = edge.from_work_item_id
               WHERE edge.to_work_item_id = ? AND edge.edge_type = 'depends_on' AND edge.active = 1
               ORDER BY wi.priority DESC, wi.created_at, wi.work_item_id"#,
        )
        .bind(work_item_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(row_to_work_item).collect()
    }
}

fn row_to_task(row: sqlx::sqlite::SqliteRow) -> Result<TaskNode> {
    Ok(TaskNode {
        task_id: row.get("task_id"),
        title: row.get("title"),
        description: row.get("description"),
        ref_: row.get("ref"),
        metadata: parse_json(row.get("metadata"))?,
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

fn row_to_work_item(row: sqlx::sqlite::SqliteRow) -> Result<WorkItemNode> {
    Ok(WorkItemNode {
        work_item_id: row.get("work_item_id"),
        task_id: row.get("task_id"),
        title: row.get("title"),
        description: row.get("description"),
        kind: row.get("kind"),
        action: row.get("action"),
        execution_profile_id: row.get("execution_profile_id"),
        execution_profile_version: row.get("execution_profile_version"),
        review_policy: parse_optional_json(row.get("review_policy"))?,
        execution_policy: parse_optional_json(row.get("execution_policy"))?,
        escalation_policy: parse_optional_json(row.get("escalation_policy"))?,
        priority: row.get("priority"),
        optional: row.get("optional"),
        parallelizable: row.get("parallelizable"),
        acceptance_criteria: parse_json(row.get("acceptance_criteria"))?,
        active: row.get("active"),
        ref_: row.get("ref"),
        metadata: parse_json(row.get("metadata"))?,
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

fn row_to_edge(row: sqlx::sqlite::SqliteRow) -> Result<WorkItemEdgeRecord> {
    let edge_type_raw: String = row.get("edge_type");
    Ok(WorkItemEdgeRecord {
        edge_id: row.get("edge_id"),
        task_id: row.get("task_id"),
        from_work_item_id: row.get("from_work_item_id"),
        to_work_item_id: row.get("to_work_item_id"),
        edge_type: GraphEdgeKind::parse(&edge_type_raw)
            .ok_or_else(|| Error::Domain(format!("unknown graph edge type {edge_type_raw}")))?,
        ref_: row.get("ref"),
        metadata: parse_json(row.get("metadata"))?,
        created_at: row.get("created_at"),
    })
}

fn row_to_signal(row: sqlx::sqlite::SqliteRow) -> Result<SignalNode> {
    Ok(SignalNode {
        signal_id: row.get("signal_id"),
        task_id: row.get("task_id"),
        work_item_id: row.get("work_item_id"),
        run_id: row.get("run_id"),
        source_session_id: row.get("source_session_id"),
        source: row.get("source"),
        kind: row.get("kind"),
        summary: row.get("summary"),
        detail: row.get("detail"),
        severity: row.get("severity"),
        related_refs: parse_json(row.get("related_refs"))?,
        state: row.get("state"),
        ref_: row.get("ref"),
        metadata: parse_json(row.get("metadata"))?,
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

fn optional_json_to_string(value: &Option<Value>) -> Result<Option<String>> {
    value
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(Into::into)
}

fn parse_json(raw: String) -> Result<Value> {
    Ok(serde_json::from_str(&raw)?)
}

fn parse_optional_json(raw: Option<String>) -> Result<Option<Value>> {
    raw.map(|value| serde_json::from_str(&value))
        .transpose()
        .map_err(Into::into)
}
