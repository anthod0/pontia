use std::path::PathBuf;

#[cfg(feature = "lbug")]
use serde_json::json;

#[cfg(feature = "lbug")]
use crate::error::Error;
use crate::error::Result;

#[cfg(feature = "lbug")]
use super::TaskProvenance;
use super::snapshot::TaskGraphSnapshot;
#[cfg(feature = "lbug")]
use super::snapshot::{decision_execution_state, decision_signal_kind, task_title};
#[cfg(feature = "lbug")]
use super::{ProvenanceEdge, ProvenanceNode};

#[cfg(not(feature = "lbug"))]
pub(super) fn project_snapshot_to_lbug(
    _db_dir: PathBuf,
    _snapshot: TaskGraphSnapshot,
) -> Result<()> {
    Ok(())
}

#[cfg(feature = "lbug")]
pub(super) fn project_snapshot_to_lbug(db_dir: PathBuf, snapshot: TaskGraphSnapshot) -> Result<()> {
    let conn = open_graph_connection(db_dir)?;
    initialize_schema(&conn)?;

    query(
        &conn,
        &format!(
            "MERGE (t:Task {{task_id: {}}}) SET t.title = {}, t.description = {}, t.ref = {}, t.created_at = {}, t.updated_at = {};",
            cypher_string(&snapshot.task_id),
            cypher_string(&task_title(&snapshot.task_input)),
            cypher_string(&snapshot.task_input),
            cypher_string(&format!("sqlite:task:{}", snapshot.task_id)),
            cypher_string(&snapshot.task_created_at),
            cypher_string(&snapshot.task_updated_at)
        ),
    )?;

    if !snapshot.decisions.is_empty() {
        query(
            &conn,
            "MERGE (a:Agent {agent_id: 'agent_planner'}) SET a.name = 'Task Planner', a.role = 'planner', a.capabilities = '[\"workspace_routing\",\"task_planning\"]', a.availability = 'available', a.ref = 'internal:planner', a.created_at = '', a.updated_at = '';",
        )?;
    }

    for decision in snapshot.decisions {
        let work_item_id = format!("wi_{}", decision.decision_id);
        let signal_id = format!("sig_{}", decision.decision_id);
        query(
            &conn,
            &format!(
                "MERGE (w:WorkItem {{work_item_id: {}}}) SET w.title = 'Plan task', w.description = {}, w.kind = 'planning', w.planning_state = 'active', w.execution_state = {}, w.execution_ref = '', w.created_at = {}, w.updated_at = {};",
                cypher_string(&work_item_id),
                cypher_string(&decision.reason),
                cypher_string(decision_execution_state(&decision.status)),
                cypher_string(&decision.created_at),
                cypher_string(&decision.created_at)
            ),
        )?;
        query(
            &conn,
            &format!(
                "MATCH (t:Task {{task_id: {}}}), (w:WorkItem {{work_item_id: {}}}) MERGE (t)-[:HAS_WORK]->(w);",
                cypher_string(&snapshot.task_id),
                cypher_string(&work_item_id)
            ),
        )?;
        query(
            &conn,
            &format!(
                "MATCH (w:WorkItem {{work_item_id: {}}}), (a:Agent {{agent_id: 'agent_planner'}}) MERGE (w)-[:ASSIGNED_TO]->(a);",
                cypher_string(&work_item_id)
            ),
        )?;
        query(
            &conn,
            &format!(
                "MERGE (s:Signal {{signal_id: {}}}) SET s.source_type = 'agent', s.kind = {}, s.summary = {}, s.detail = {}, s.origin_ref = {}, s.created_at = {};",
                cypher_string(&signal_id),
                cypher_string(decision_signal_kind(&decision.status)),
                cypher_string(&decision.reason),
                cypher_string(&format!(
                    "planner status: {}; confidence: {}",
                    decision.status, decision.confidence
                )),
                cypher_string(&format!("sqlite:task:{}", snapshot.task_id)),
                cypher_string(&decision.created_at)
            ),
        )?;
        query(
            &conn,
            &format!(
                "MATCH (t:Task {{task_id: {}}}), (s:Signal {{signal_id: {}}}) MERGE (t)-[:HAS_SIGNAL]->(s);",
                cypher_string(&snapshot.task_id),
                cypher_string(&signal_id)
            ),
        )?;
        query(
            &conn,
            &format!(
                "MATCH (a:Agent {{agent_id: 'agent_planner'}}), (s:Signal {{signal_id: {}}}) MERGE (a)-[:EMITS]->(s);",
                cypher_string(&signal_id)
            ),
        )?;

        for evidence in decision.evidence {
            let artifact_id = format!("art_{}", evidence.evidence_id);
            query(
                &conn,
                &format!(
                    "MERGE (a:Artifact {{artifact_id: {}}}) SET a.kind = {}, a.name = {}, a.summary = {}, a.availability = 'available', a.ref = {}, a.created_at = '', a.updated_at = '';",
                    cypher_string(&artifact_id),
                    cypher_string(&evidence.kind),
                    cypher_string(&evidence.evidence_id),
                    cypher_string(&evidence.summary),
                    cypher_string(&evidence.reference)
                ),
            )?;
            query(
                &conn,
                &format!(
                    "MATCH (w:WorkItem {{work_item_id: {}}}), (a:Artifact {{artifact_id: {}}}) MERGE (w)-[:REQUIRES]->(a);",
                    cypher_string(&work_item_id),
                    cypher_string(&artifact_id)
                ),
            )?;
            query(
                &conn,
                &format!(
                    "MATCH (s:Signal {{signal_id: {}}}), (a:Artifact {{artifact_id: {}}}) MERGE (s)-[:SUPPORTED_BY]->(a);",
                    cypher_string(&signal_id),
                    cypher_string(&artifact_id)
                ),
            )?;
        }
    }

    Ok(())
}

#[cfg(feature = "lbug")]
pub(super) fn query_task_provenance(db_dir: PathBuf, task_id: &str) -> Result<TaskProvenance> {
    let conn = open_graph_connection(db_dir)?;
    initialize_schema(&conn)?;

    let mut nodes = Vec::new();
    let mut edges = Vec::new();

    let mut task_rows = query(
        &conn,
        &format!(
            "MATCH (t:Task {{task_id: {}}}) RETURN t.task_id, t.title, t.description, t.ref, t.created_at, t.updated_at;",
            cypher_string(task_id)
        ),
    )?;
    for row in task_rows.by_ref() {
        push_unique_node(
            &mut nodes,
            ProvenanceNode {
                id: string_value(&row[0]),
                kind: "Task".to_string(),
                properties: json!({
                    "title": string_value(&row[1]),
                    "description": string_value(&row[2]),
                    "ref": string_value(&row[3]),
                    "created_at": string_value(&row[4]),
                    "updated_at": string_value(&row[5])
                }),
            },
        );
    }

    let mut work_rows = query(
        &conn,
        &format!(
            "MATCH (t:Task {{task_id: {}}})-[r:HAS_WORK]->(w:WorkItem) RETURN t.task_id, w.work_item_id, w.title, w.description, w.kind, w.planning_state, w.execution_state, w.execution_ref, w.created_at, w.updated_at;",
            cypher_string(task_id)
        ),
    )?;
    for row in work_rows.by_ref() {
        edges.push(ProvenanceEdge {
            from: string_value(&row[0]),
            to: string_value(&row[1]),
            kind: "HAS_WORK".to_string(),
            properties: json!({}),
        });
        push_unique_node(
            &mut nodes,
            ProvenanceNode {
                id: string_value(&row[1]),
                kind: "WorkItem".to_string(),
                properties: json!({
                    "title": string_value(&row[2]),
                    "description": string_value(&row[3]),
                    "kind": string_value(&row[4]),
                    "planning_state": string_value(&row[5]),
                    "execution_state": string_value(&row[6]),
                    "execution_ref": string_value(&row[7]),
                    "created_at": string_value(&row[8]),
                    "updated_at": string_value(&row[9])
                }),
            },
        );
    }

    let mut signal_rows = query(
        &conn,
        &format!(
            "MATCH (t:Task {{task_id: {}}})-[r:HAS_SIGNAL]->(s:Signal) RETURN t.task_id, s.signal_id, s.source_type, s.kind, s.summary, s.detail, s.origin_ref, s.created_at;",
            cypher_string(task_id)
        ),
    )?;
    for row in signal_rows.by_ref() {
        edges.push(ProvenanceEdge {
            from: string_value(&row[0]),
            to: string_value(&row[1]),
            kind: "HAS_SIGNAL".to_string(),
            properties: json!({}),
        });
        push_unique_node(
            &mut nodes,
            ProvenanceNode {
                id: string_value(&row[1]),
                kind: "Signal".to_string(),
                properties: json!({
                    "source_type": string_value(&row[2]),
                    "kind": string_value(&row[3]),
                    "summary": string_value(&row[4]),
                    "detail": string_value(&row[5]),
                    "origin_ref": string_value(&row[6]),
                    "created_at": string_value(&row[7])
                }),
            },
        );
    }

    let mut assignment_rows = query(
        &conn,
        &format!(
            "MATCH (t:Task {{task_id: {}}})-[:HAS_WORK]->(w:WorkItem)-[r:ASSIGNED_TO]->(a:Agent) RETURN w.work_item_id, a.agent_id, a.name, a.role, a.capabilities, a.availability, a.ref, a.created_at, a.updated_at;",
            cypher_string(task_id)
        ),
    )?;
    for row in assignment_rows.by_ref() {
        edges.push(ProvenanceEdge {
            from: string_value(&row[0]),
            to: string_value(&row[1]),
            kind: "ASSIGNED_TO".to_string(),
            properties: json!({}),
        });
        push_unique_node(
            &mut nodes,
            ProvenanceNode {
                id: string_value(&row[1]),
                kind: "Agent".to_string(),
                properties: json!({
                    "name": string_value(&row[2]),
                    "role": string_value(&row[3]),
                    "capabilities": string_value(&row[4]),
                    "availability": string_value(&row[5]),
                    "ref": string_value(&row[6]),
                    "created_at": string_value(&row[7]),
                    "updated_at": string_value(&row[8])
                }),
            },
        );
    }

    let mut require_rows = query(
        &conn,
        &format!(
            "MATCH (t:Task {{task_id: {}}})-[:HAS_WORK]->(w:WorkItem)-[r:REQUIRES]->(a:Artifact) RETURN w.work_item_id, a.artifact_id, a.kind, a.name, a.summary, a.availability, a.ref, a.created_at, a.updated_at;",
            cypher_string(task_id)
        ),
    )?;
    for row in require_rows.by_ref() {
        edges.push(ProvenanceEdge {
            from: string_value(&row[0]),
            to: string_value(&row[1]),
            kind: "REQUIRES".to_string(),
            properties: json!({}),
        });
        push_unique_node(&mut nodes, artifact_node_from_row(&row, 1));
    }

    let mut emit_rows = query(
        &conn,
        &format!(
            "MATCH (t:Task {{task_id: {}}})-[:HAS_SIGNAL]->(s:Signal)<-[r:EMITS]-(a:Agent) RETURN a.agent_id, s.signal_id, a.name, a.role, a.capabilities, a.availability, a.ref, a.created_at, a.updated_at;",
            cypher_string(task_id)
        ),
    )?;
    for row in emit_rows.by_ref() {
        edges.push(ProvenanceEdge {
            from: string_value(&row[0]),
            to: string_value(&row[1]),
            kind: "EMITS".to_string(),
            properties: json!({}),
        });
        push_unique_node(
            &mut nodes,
            ProvenanceNode {
                id: string_value(&row[0]),
                kind: "Agent".to_string(),
                properties: json!({
                    "name": string_value(&row[2]),
                    "role": string_value(&row[3]),
                    "capabilities": string_value(&row[4]),
                    "availability": string_value(&row[5]),
                    "ref": string_value(&row[6]),
                    "created_at": string_value(&row[7]),
                    "updated_at": string_value(&row[8])
                }),
            },
        );
    }

    let mut support_rows = query(
        &conn,
        &format!(
            "MATCH (t:Task {{task_id: {}}})-[:HAS_SIGNAL]->(s:Signal)-[r:SUPPORTED_BY]->(a:Artifact) RETURN s.signal_id, a.artifact_id, a.kind, a.name, a.summary, a.availability, a.ref, a.created_at, a.updated_at;",
            cypher_string(task_id)
        ),
    )?;
    for row in support_rows.by_ref() {
        edges.push(ProvenanceEdge {
            from: string_value(&row[0]),
            to: string_value(&row[1]),
            kind: "SUPPORTED_BY".to_string(),
            properties: json!({}),
        });
        push_unique_node(&mut nodes, artifact_node_from_row(&row, 1));
    }

    Ok(TaskProvenance { nodes, edges })
}

#[cfg(feature = "lbug")]
fn open_graph_connection(db_dir: PathBuf) -> Result<lbug::Connection<'static>> {
    if let Some(parent) = db_dir
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)?;
    }
    let config = lbug::SystemConfig::default().enable_multi_writes(true);
    let db = lbug::Database::new(db_dir, config)
        .map_err(|error| Error::Domain(format!("lbug database open failed: {error}")))?;
    let db = Box::leak(Box::new(db));
    lbug::Connection::new(db)
        .map_err(|error| Error::Domain(format!("lbug connection failed: {error}")))
}

#[cfg(feature = "lbug")]
fn initialize_schema<'db>(conn: &lbug::Connection<'db>) -> Result<()> {
    for statement in [
        "CREATE NODE TABLE IF NOT EXISTS Task(task_id STRING, title STRING, description STRING, ref STRING, created_at STRING, updated_at STRING, PRIMARY KEY(task_id));",
        "CREATE NODE TABLE IF NOT EXISTS WorkItem(work_item_id STRING, title STRING, description STRING, kind STRING, planning_state STRING, execution_state STRING, execution_ref STRING, created_at STRING, updated_at STRING, PRIMARY KEY(work_item_id));",
        "CREATE NODE TABLE IF NOT EXISTS Agent(agent_id STRING, name STRING, role STRING, capabilities STRING, availability STRING, ref STRING, created_at STRING, updated_at STRING, PRIMARY KEY(agent_id));",
        "CREATE NODE TABLE IF NOT EXISTS Artifact(artifact_id STRING, kind STRING, name STRING, summary STRING, availability STRING, ref STRING, created_at STRING, updated_at STRING, PRIMARY KEY(artifact_id));",
        "CREATE NODE TABLE IF NOT EXISTS Signal(signal_id STRING, source_type STRING, kind STRING, summary STRING, detail STRING, origin_ref STRING, created_at STRING, PRIMARY KEY(signal_id));",
        "CREATE REL TABLE IF NOT EXISTS HAS_WORK(FROM Task TO WorkItem);",
        "CREATE REL TABLE IF NOT EXISTS HAS_SIGNAL(FROM Task TO Signal);",
        "CREATE REL TABLE IF NOT EXISTS DEPENDS_ON(FROM WorkItem TO WorkItem);",
        "CREATE REL TABLE IF NOT EXISTS SUPERSEDES(FROM WorkItem TO WorkItem);",
        "CREATE REL TABLE IF NOT EXISTS CAUSED_BY(FROM WorkItem TO Signal);",
        "CREATE REL TABLE IF NOT EXISTS ASSIGNED_TO(FROM WorkItem TO Agent);",
        "CREATE REL TABLE IF NOT EXISTS REQUIRES(FROM WorkItem TO Artifact);",
        "CREATE REL TABLE IF NOT EXISTS PRODUCES(FROM WorkItem TO Artifact);",
        "CREATE REL TABLE IF NOT EXISTS EMITS(FROM Agent TO Signal);",
        "CREATE REL TABLE IF NOT EXISTS SUPPORTED_BY(FROM Signal TO Artifact);",
        "CREATE REL TABLE IF NOT EXISTS DERIVED_FROM(FROM Artifact TO Artifact);",
    ] {
        query(conn, statement)?;
    }
    Ok(())
}

#[cfg(feature = "lbug")]
fn query<'db>(conn: &lbug::Connection<'db>, statement: &str) -> Result<lbug::QueryResult<'db>> {
    conn.query(statement)
        .map_err(|error| Error::Domain(format!("lbug query failed: {error}; query: {statement}")))
}

#[cfg(feature = "lbug")]
fn cypher_string(value: &str) -> String {
    format!("'{}'", value.replace('\\', "\\\\").replace('\'', "\\'"))
}

#[cfg(feature = "lbug")]
fn string_value(value: &lbug::Value) -> String {
    match value {
        lbug::Value::String(value) => value.clone(),
        lbug::Value::Null(_) => String::new(),
        other => other.to_string(),
    }
}

#[cfg(feature = "lbug")]
fn artifact_node_from_row(row: &[lbug::Value], offset: usize) -> ProvenanceNode {
    ProvenanceNode {
        id: string_value(&row[offset]),
        kind: "Artifact".to_string(),
        properties: json!({
            "kind": string_value(&row[offset + 1]),
            "name": string_value(&row[offset + 2]),
            "summary": string_value(&row[offset + 3]),
            "availability": string_value(&row[offset + 4]),
            "ref": string_value(&row[offset + 5]),
            "created_at": string_value(&row[offset + 6]),
            "updated_at": string_value(&row[offset + 7])
        }),
    }
}

#[cfg(feature = "lbug")]
fn push_unique_node(nodes: &mut Vec<ProvenanceNode>, node: ProvenanceNode) {
    if !nodes
        .iter()
        .any(|existing| existing.id == node.id && existing.kind == node.kind)
    {
        nodes.push(node);
    }
}
