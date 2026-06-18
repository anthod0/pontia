#![cfg(feature = "lbug")]

mod params;
mod path;
mod rows;
mod schema;
mod values;

use params::{edge_params, signal_params, work_item_params};
use path::expand_home_prefix;
use rows::{row_to_edge, row_to_signal, row_to_task, row_to_work_item};
use schema::LBUG_SCHEMA;
use std::{path::Path, sync::Arc};
use values::{json_value, now_string, optional_string_value, rel_label, string_value};

use lbug::{Connection, Database, SystemConfig, Value as LbugValue};
use uuid::Uuid;

use pontia_core::error::Result;

use super::{
    AddWorkItemEdgeRequest, GraphEdgeKind, SignalNode, TaskGraphSnapshot, TaskNode,
    UpsertSignalRequest, UpsertTaskRequest, UpsertWorkItemRequest, WorkItemEdgeRecord,
    WorkItemNode,
};

#[derive(Debug, Clone)]
pub struct LbugDagGraphStore {
    db: Arc<Database>,
}

impl LbugDagGraphStore {
    pub async fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = expand_home_prefix(path.as_ref());
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)?;
        }
        let db = Database::new(&path, SystemConfig::default())?;
        let store = Self { db: Arc::new(db) };
        store.initialize_schema()?;
        Ok(store)
    }

    pub async fn upsert_task(&self, request: UpsertTaskRequest) -> Result<()> {
        let conn = self.connection()?;
        let now = now_string();
        if self.task_exists_with_conn(&conn, &request.task_id)? {
            let mut statement = conn.prepare(
                "MATCH (t:Task) WHERE t.task_id = $task_id \
                 SET t.title = $title, t.description = $description, t.ref = $ref, \
                     t.metadata = $metadata, t.updated_at = $updated_at",
            )?;
            conn.execute(
                &mut statement,
                vec![
                    ("task_id", string_value(request.task_id)),
                    ("title", string_value(request.title)),
                    ("description", string_value(request.description)),
                    ("ref", optional_string_value(request.ref_)),
                    ("metadata", json_value(request.metadata)?),
                    ("updated_at", string_value(now)),
                ],
            )?;
        } else {
            let mut statement = conn.prepare(
                "CREATE (:Task { task_id: $task_id, title: $title, description: $description, \
                    ref: $ref, metadata: $metadata, created_at: $created_at, updated_at: $updated_at })",
            )?;
            conn.execute(
                &mut statement,
                vec![
                    ("task_id", string_value(request.task_id)),
                    ("title", string_value(request.title)),
                    ("description", string_value(request.description)),
                    ("ref", optional_string_value(request.ref_)),
                    ("metadata", json_value(request.metadata)?),
                    ("created_at", string_value(now.clone())),
                    ("updated_at", string_value(now)),
                ],
            )?;
        }
        Ok(())
    }

    pub async fn upsert_work_item(&self, request: UpsertWorkItemRequest) -> Result<()> {
        let conn = self.connection()?;
        let now = now_string();
        if self.work_item_exists_with_conn(&conn, &request.work_item_id)? {
            let mut statement = conn.prepare(
                "MATCH (wi:WorkItem) WHERE wi.work_item_id = $work_item_id \
                 SET wi.task_id = $task_id, wi.title = $title, wi.description = $description, \
                     wi.kind = $kind, wi.action = $action, \
                     wi.execution_profile_id = $execution_profile_id, \
                     wi.execution_profile_version = $execution_profile_version, \
                     wi.review_policy = $review_policy, wi.execution_policy = $execution_policy, \
                     wi.escalation_policy = $escalation_policy, wi.priority = $priority, \
                     wi.optional_flag = $optional_value, wi.parallelizable = $parallelizable, \
                     wi.acceptance_criteria = $acceptance_criteria, wi.active = $active, \
                     wi.ref = $ref, wi.metadata = $metadata, wi.updated_at = $updated_at",
            )?;
            conn.execute(
                &mut statement,
                work_item_params(request.clone(), Some(now))?,
            )?;
        } else {
            let mut statement = conn.prepare(
                "CREATE (:WorkItem { work_item_id: $work_item_id, task_id: $task_id, \
                    title: $title, description: $description, kind: $kind, action: $action, \
                    execution_profile_id: $execution_profile_id, \
                    execution_profile_version: $execution_profile_version, \
                    review_policy: $review_policy, execution_policy: $execution_policy, \
                    escalation_policy: $escalation_policy, priority: $priority, optional_flag: $optional_value, \
                    parallelizable: $parallelizable, acceptance_criteria: $acceptance_criteria, \
                    active: $active, ref: $ref, metadata: $metadata, \
                    created_at: $created_at, updated_at: $updated_at })",
            )?;
            let mut params = work_item_params(request.clone(), Some(now.clone()))?;
            params.push(("created_at", string_value(now)));
            conn.execute(&mut statement, params)?;
        }
        self.ensure_has_work(&conn, &request.task_id, &request.work_item_id)?;
        Ok(())
    }

    pub async fn set_work_item_active(&self, work_item_id: &str, active: bool) -> Result<()> {
        let conn = self.connection()?;
        let mut statement = conn.prepare(
            "MATCH (wi:WorkItem) WHERE wi.work_item_id = $work_item_id \
             SET wi.active = $active, wi.updated_at = $updated_at",
        )?;
        conn.execute(
            &mut statement,
            vec![
                ("work_item_id", string_value(work_item_id)),
                ("active", LbugValue::Bool(active)),
                ("updated_at", string_value(now_string())),
            ],
        )?;
        Ok(())
    }

    pub async fn add_edge(&self, request: AddWorkItemEdgeRequest) -> Result<()> {
        let conn = self.connection()?;
        if self.edge_exists_with_conn(&conn, &request)? {
            let query = format!(
                "MATCH (from:WorkItem)-[edge:{}]->(to:WorkItem) \
                 WHERE edge.task_id = $task_id AND from.work_item_id = $from_work_item_id \
                   AND to.work_item_id = $to_work_item_id \
                 SET edge.active = true, edge.ref = $ref",
                rel_label(request.edge_type)
            );
            let mut statement = conn.prepare(&query)?;
            conn.execute(&mut statement, edge_params(&request, None))?;
        } else {
            let query = format!(
                "MATCH (from:WorkItem), (to:WorkItem) \
                 WHERE from.work_item_id = $from_work_item_id AND to.work_item_id = $to_work_item_id \
                 CREATE (from)-[:{} {{ edge_id: $edge_id, task_id: $task_id, ref: $ref, \
                    metadata: $metadata, active: true, created_at: $created_at }}]->(to)",
                rel_label(request.edge_type)
            );
            let mut statement = conn.prepare(&query)?;
            conn.execute(
                &mut statement,
                edge_params(&request, Some(format!("gie_{}", Uuid::now_v7()))),
            )?;
        }
        Ok(())
    }

    pub async fn remove_edge(
        &self,
        task_id: &str,
        from_work_item_id: &str,
        to_work_item_id: &str,
        edge_type: GraphEdgeKind,
    ) -> Result<()> {
        let conn = self.connection()?;
        let query = format!(
            "MATCH (from:WorkItem)-[edge:{}]->(to:WorkItem) \
             WHERE edge.task_id = $task_id AND from.work_item_id = $from_work_item_id \
               AND to.work_item_id = $to_work_item_id \
             SET edge.active = false",
            rel_label(edge_type)
        );
        let mut statement = conn.prepare(&query)?;
        conn.execute(
            &mut statement,
            vec![
                ("task_id", string_value(task_id)),
                ("from_work_item_id", string_value(from_work_item_id)),
                ("to_work_item_id", string_value(to_work_item_id)),
            ],
        )?;
        Ok(())
    }

    pub async fn upsert_signal(&self, request: UpsertSignalRequest) -> Result<()> {
        let conn = self.connection()?;
        let now = now_string();
        if self.signal_exists_with_conn(&conn, &request.signal_id)? {
            let mut statement = conn.prepare(
                "MATCH (sig:Signal) WHERE sig.signal_id = $signal_id \
                 SET sig.task_id = $task_id, sig.work_item_id = $work_item_id, sig.run_id = $run_id, \
                     sig.source_session_id = $source_session_id, sig.source = $source, \
                     sig.kind = $kind, sig.summary = $summary, sig.detail = $detail, \
                     sig.severity = $severity, sig.related_refs = $related_refs, sig.state = $state, \
                     sig.ref = $ref, sig.metadata = $metadata, sig.updated_at = $updated_at",
            )?;
            conn.execute(&mut statement, signal_params(request.clone(), Some(now))?)?;
        } else {
            let mut statement = conn.prepare(
                "CREATE (:Signal { signal_id: $signal_id, task_id: $task_id, \
                    work_item_id: $work_item_id, run_id: $run_id, source_session_id: $source_session_id, \
                    source: $source, kind: $kind, summary: $summary, detail: $detail, \
                    severity: $severity, related_refs: $related_refs, state: $state, ref: $ref, \
                    metadata: $metadata, created_at: $created_at, updated_at: $updated_at })",
            )?;
            let mut params = signal_params(request.clone(), Some(now.clone()))?;
            params.push(("created_at", string_value(now)));
            conn.execute(&mut statement, params)?;
        }
        self.ensure_has_signal(&conn, &request.task_id, &request.signal_id)?;
        Ok(())
    }

    pub async fn task_graph(&self, task_id: &str) -> Result<TaskGraphSnapshot> {
        let conn = self.connection()?;
        Ok(TaskGraphSnapshot {
            task: self.fetch_task(&conn, task_id)?,
            work_items: self.fetch_work_items(&conn, task_id)?,
            edges: self.fetch_edges(&conn, task_id)?,
            signals: self.fetch_signals(&conn, task_id)?,
        })
    }

    pub async fn get_work_item(&self, work_item_id: &str) -> Result<Option<WorkItemNode>> {
        let conn = self.connection()?;
        let mut statement = conn.prepare(
            "MATCH (wi:WorkItem) WHERE wi.work_item_id = $work_item_id \
             RETURN wi.work_item_id, wi.task_id, wi.title, wi.description, wi.kind, wi.action, \
                    wi.execution_profile_id, wi.execution_profile_version, wi.review_policy, \
                    wi.execution_policy, wi.escalation_policy, wi.priority, wi.optional_flag, \
                    wi.parallelizable, wi.acceptance_criteria, wi.active, wi.ref, wi.metadata, \
                    wi.created_at, wi.updated_at",
        )?;
        let mut result = conn.execute(
            &mut statement,
            vec![("work_item_id", string_value(work_item_id))],
        )?;
        result.next().map(row_to_work_item).transpose()
    }

    pub async fn list_dependencies(&self, work_item_id: &str) -> Result<Vec<WorkItemNode>> {
        let conn = self.connection()?;
        let mut statement = conn.prepare(
            "MATCH (dep:WorkItem)-[edge:DEPENDS_ON]->(wi:WorkItem) \
             WHERE wi.work_item_id = $work_item_id AND edge.active = true \
             RETURN dep.work_item_id, dep.task_id, dep.title, dep.description, dep.kind, dep.action, \
                    dep.execution_profile_id, dep.execution_profile_version, dep.review_policy, \
                    dep.execution_policy, dep.escalation_policy, dep.priority, dep.optional_flag, \
                    dep.parallelizable, dep.acceptance_criteria, dep.active, dep.ref, dep.metadata, \
                    dep.created_at, dep.updated_at \
             ORDER BY dep.priority DESC, dep.created_at, dep.work_item_id",
        )?;
        let result = conn.execute(
            &mut statement,
            vec![("work_item_id", string_value(work_item_id))],
        )?;
        result.map(row_to_work_item).collect()
    }

    fn initialize_schema(&self) -> Result<()> {
        let conn = self.connection()?;
        for statement in LBUG_SCHEMA {
            conn.query(statement)?;
        }
        Ok(())
    }

    fn connection(&self) -> Result<Connection<'_>> {
        Ok(Connection::new(self.db.as_ref())?)
    }

    fn task_exists_with_conn(&self, conn: &Connection<'_>, task_id: &str) -> Result<bool> {
        exists(
            conn,
            "MATCH (t:Task) WHERE t.task_id = $id RETURN t.task_id",
            task_id,
        )
    }

    fn work_item_exists_with_conn(
        &self,
        conn: &Connection<'_>,
        work_item_id: &str,
    ) -> Result<bool> {
        exists(
            conn,
            "MATCH (wi:WorkItem) WHERE wi.work_item_id = $id RETURN wi.work_item_id",
            work_item_id,
        )
    }

    fn signal_exists_with_conn(&self, conn: &Connection<'_>, signal_id: &str) -> Result<bool> {
        exists(
            conn,
            "MATCH (sig:Signal) WHERE sig.signal_id = $id RETURN sig.signal_id",
            signal_id,
        )
    }

    fn edge_exists_with_conn(
        &self,
        conn: &Connection<'_>,
        request: &AddWorkItemEdgeRequest,
    ) -> Result<bool> {
        let query = format!(
            "MATCH (from:WorkItem)-[edge:{}]->(to:WorkItem) \
             WHERE edge.task_id = $task_id AND from.work_item_id = $from_work_item_id \
               AND to.work_item_id = $to_work_item_id \
             RETURN edge.edge_id",
            rel_label(request.edge_type)
        );
        let mut statement = conn.prepare(&query)?;
        let mut result = conn.execute(
            &mut statement,
            vec![
                ("task_id", string_value(&request.task_id)),
                (
                    "from_work_item_id",
                    string_value(&request.from_work_item_id),
                ),
                ("to_work_item_id", string_value(&request.to_work_item_id)),
            ],
        )?;
        Ok(result.next().is_some())
    }

    fn ensure_has_work(
        &self,
        conn: &Connection<'_>,
        task_id: &str,
        work_item_id: &str,
    ) -> Result<()> {
        let mut statement = conn.prepare(
            "MATCH (task:Task), (wi:WorkItem) \
             WHERE task.task_id = $task_id AND wi.work_item_id = $work_item_id \
             CREATE (task)-[:HAS_WORK]->(wi)",
        )?;
        conn.execute(
            &mut statement,
            vec![
                ("task_id", string_value(task_id)),
                ("work_item_id", string_value(work_item_id)),
            ],
        )?;
        Ok(())
    }

    fn ensure_has_signal(
        &self,
        conn: &Connection<'_>,
        task_id: &str,
        signal_id: &str,
    ) -> Result<()> {
        let mut statement = conn.prepare(
            "MATCH (task:Task), (sig:Signal) \
             WHERE task.task_id = $task_id AND sig.signal_id = $signal_id \
             CREATE (task)-[:HAS_SIGNAL]->(sig)",
        )?;
        conn.execute(
            &mut statement,
            vec![
                ("task_id", string_value(task_id)),
                ("signal_id", string_value(signal_id)),
            ],
        )?;
        Ok(())
    }

    fn fetch_task(&self, conn: &Connection<'_>, task_id: &str) -> Result<Option<TaskNode>> {
        let mut statement = conn.prepare(
            "MATCH (t:Task) WHERE t.task_id = $task_id \
             RETURN t.task_id, t.title, t.description, t.ref, t.metadata, t.created_at, t.updated_at",
        )?;
        let mut result = conn.execute(&mut statement, vec![("task_id", string_value(task_id))])?;
        result.next().map(row_to_task).transpose()
    }

    fn fetch_work_items(&self, conn: &Connection<'_>, task_id: &str) -> Result<Vec<WorkItemNode>> {
        let mut statement = conn.prepare(
            "MATCH (wi:WorkItem) WHERE wi.task_id = $task_id \
             RETURN wi.work_item_id, wi.task_id, wi.title, wi.description, wi.kind, wi.action, \
                    wi.execution_profile_id, wi.execution_profile_version, wi.review_policy, \
                    wi.execution_policy, wi.escalation_policy, wi.priority, wi.optional_flag, \
                    wi.parallelizable, wi.acceptance_criteria, wi.active, wi.ref, wi.metadata, \
                    wi.created_at, wi.updated_at \
             ORDER BY wi.priority DESC, wi.created_at, wi.work_item_id",
        )?;
        let result = conn.execute(&mut statement, vec![("task_id", string_value(task_id))])?;
        result.map(row_to_work_item).collect()
    }

    fn fetch_edges(&self, conn: &Connection<'_>, task_id: &str) -> Result<Vec<WorkItemEdgeRecord>> {
        let mut edges = Vec::new();
        for edge_type in [
            GraphEdgeKind::DependsOn,
            GraphEdgeKind::Reviews,
            GraphEdgeKind::Supersedes,
            GraphEdgeKind::CausedBy,
        ] {
            let query = format!(
                "MATCH (from:WorkItem)-[edge:{}]->(to:WorkItem) \
                 WHERE edge.task_id = $task_id AND edge.active = true \
                 RETURN edge.edge_id, edge.task_id, from.work_item_id, to.work_item_id, \
                        edge.ref, edge.metadata, edge.created_at \
                 ORDER BY edge.created_at, edge.edge_id",
                rel_label(edge_type)
            );
            let mut statement = conn.prepare(&query)?;
            let result = conn.execute(&mut statement, vec![("task_id", string_value(task_id))])?;
            for row in result {
                edges.push(row_to_edge(row, edge_type)?);
            }
        }
        edges.sort_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then(left.edge_id.cmp(&right.edge_id))
        });
        Ok(edges)
    }

    fn fetch_signals(&self, conn: &Connection<'_>, task_id: &str) -> Result<Vec<SignalNode>> {
        let mut statement = conn.prepare(
            "MATCH (sig:Signal) WHERE sig.task_id = $task_id \
             RETURN sig.signal_id, sig.task_id, sig.work_item_id, sig.run_id, sig.source_session_id, \
                    sig.source, sig.kind, sig.summary, sig.detail, sig.severity, sig.related_refs, \
                    sig.state, sig.ref, sig.metadata, sig.created_at, sig.updated_at \
             ORDER BY sig.created_at, sig.signal_id",
        )?;
        let result = conn.execute(&mut statement, vec![("task_id", string_value(task_id))])?;
        result.map(row_to_signal).collect()
    }
}
fn exists(conn: &Connection<'_>, query: &str, id: &str) -> Result<bool> {
    let mut statement = conn.prepare(query)?;
    let mut result = conn.execute(&mut statement, vec![("id", string_value(id))])?;
    Ok(result.next().is_some())
}
