use super::*;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GraphRuntimeConfig {
    pub enabled: bool,
    pub db_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ProvenanceNode {
    pub id: String,
    pub kind: String,
    pub properties: Value,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ProvenanceEdge {
    pub from: String,
    pub to: String,
    pub kind: String,
    pub properties: Value,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TaskProvenance {
    pub nodes: Vec<ProvenanceNode>,
    pub edges: Vec<ProvenanceEdge>,
}

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
        project_snapshot_to_kuzu(PathBuf::from(db_dir), snapshot)
    }

    pub async fn task_provenance(&self, task_id: &str) -> Result<TaskProvenance> {
        if !self.config.enabled {
            return Ok(TaskProvenance {
                nodes: vec![],
                edges: vec![],
            });
        }

        #[cfg(feature = "kuzu")]
        if let Some(db_dir) = self.config.db_dir.clone() {
            return query_task_provenance(PathBuf::from(db_dir), task_id);
        }

        let snapshot = self.load_task_snapshot(task_id).await?;
        Ok(snapshot_to_provenance(snapshot))
    }

    async fn load_task_snapshot(&self, task_id: &str) -> Result<TaskGraphSnapshot> {
        let task_row = sqlx::query(
            r#"SELECT task_id, state, workspace_id, session_id, turn_id
               FROM tasks WHERE task_id = ?"#,
        )
        .bind(task_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| Error::NotFound(format!("task {task_id} not found")))?;

        let workspace_id: Option<String> = task_row.try_get("workspace_id")?;
        let session_id: Option<String> = task_row.try_get("session_id")?;
        let turn_id: Option<String> = task_row.try_get("turn_id")?;

        let workspace = if let Some(workspace_id) = workspace_id.as_deref() {
            sqlx::query(
                "SELECT workspace_id, canonical_path FROM workspaces WHERE workspace_id = ?",
            )
            .bind(workspace_id)
            .fetch_optional(&self.pool)
            .await?
            .map(|row| {
                Ok::<GraphWorkspace, Error>(GraphWorkspace {
                    workspace_id: row.try_get("workspace_id")?,
                    canonical_path: row.try_get("canonical_path")?,
                })
            })
            .transpose()?
        } else {
            None
        };

        let session = if let Some(session_id) = session_id.as_deref() {
            sqlx::query("SELECT session_id, client_type FROM sessions WHERE session_id = ?")
                .bind(session_id)
                .fetch_optional(&self.pool)
                .await?
                .map(|row| {
                    Ok::<GraphSession, Error>(GraphSession {
                        session_id: row.try_get("session_id")?,
                        client_type: row.try_get("client_type")?,
                    })
                })
                .transpose()?
        } else {
            None
        };

        let turn = if let Some(turn_id) = turn_id.as_deref() {
            sqlx::query("SELECT turn_id, state FROM turns WHERE turn_id = ?")
                .bind(turn_id)
                .fetch_optional(&self.pool)
                .await?
                .map(|row| {
                    Ok::<GraphTurn, Error>(GraphTurn {
                        turn_id: row.try_get("turn_id")?,
                        state: row.try_get("state")?,
                    })
                })
                .transpose()?
        } else {
            None
        };

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
            task_state: task_row.try_get("state")?,
            workspace,
            session,
            turn,
            decisions,
        })
    }
}

#[derive(Debug)]
struct TaskGraphSnapshot {
    task_id: String,
    task_state: String,
    workspace: Option<GraphWorkspace>,
    session: Option<GraphSession>,
    turn: Option<GraphTurn>,
    decisions: Vec<GraphDecision>,
}

#[derive(Debug)]
struct GraphWorkspace {
    workspace_id: String,
    canonical_path: String,
}

#[derive(Debug)]
struct GraphSession {
    session_id: String,
    client_type: String,
}

#[derive(Debug)]
struct GraphTurn {
    turn_id: String,
    state: String,
}

#[derive(Debug)]
struct GraphDecision {
    decision_id: String,
    status: String,
    reason: String,
    confidence: f64,
    created_at: String,
    evidence: Vec<GraphEvidence>,
}

#[derive(Debug)]
struct GraphEvidence {
    evidence_id: String,
    kind: String,
    reference: String,
    summary: String,
}

#[cfg(not(feature = "kuzu"))]
fn project_snapshot_to_kuzu(_db_dir: PathBuf, _snapshot: TaskGraphSnapshot) -> Result<()> {
    Ok(())
}

#[cfg(feature = "kuzu")]
fn project_snapshot_to_kuzu(db_dir: PathBuf, snapshot: TaskGraphSnapshot) -> Result<()> {
    let conn = open_graph_connection(db_dir)?;
    initialize_schema(&conn)?;

    query(
        &conn,
        &format!(
            "MERGE (t:Task {{task_id: {}}}) SET t.state = {};",
            cypher_string(&snapshot.task_id),
            cypher_string(&snapshot.task_state)
        ),
    )?;

    if let Some(workspace) = snapshot.workspace {
        query(
            &conn,
            &format!(
                "MERGE (w:Workspace {{workspace_id: {}}}) SET w.canonical_path = {};",
                cypher_string(&workspace.workspace_id),
                cypher_string(&workspace.canonical_path)
            ),
        )?;
        query(
            &conn,
            &format!(
                "MATCH (t:Task {{task_id: {}}}), (w:Workspace {{workspace_id: {}}}) MERGE (t)-[:ROUTED_TO]->(w);",
                cypher_string(&snapshot.task_id),
                cypher_string(&workspace.workspace_id)
            ),
        )?;
    }

    if let Some(session) = snapshot.session {
        query(
            &conn,
            &format!(
                "MERGE (s:Session {{session_id: {}}}) SET s.client_type = {};",
                cypher_string(&session.session_id),
                cypher_string(&session.client_type)
            ),
        )?;
        query(
            &conn,
            &format!(
                "MATCH (t:Task {{task_id: {}}}), (s:Session {{session_id: {}}}) MERGE (t)-[:DISPATCHED_TO]->(s);",
                cypher_string(&snapshot.task_id),
                cypher_string(&session.session_id)
            ),
        )?;
        if let Some(turn) = snapshot.turn {
            query(
                &conn,
                &format!(
                    "MERGE (u:Turn {{turn_id: {}}}) SET u.state = {};",
                    cypher_string(&turn.turn_id),
                    cypher_string(&turn.state)
                ),
            )?;
            query(
                &conn,
                &format!(
                    "MATCH (s:Session {{session_id: {}}}), (u:Turn {{turn_id: {}}}) MERGE (s)-[:HAS_TURN]->(u);",
                    cypher_string(&session.session_id),
                    cypher_string(&turn.turn_id)
                ),
            )?;
        }
    }

    for decision in snapshot.decisions {
        query(
            &conn,
            &format!(
                "MERGE (d:Decision {{decision_id: {}}}) SET d.status = {}, d.reason = {}, d.confidence = {}, d.created_at = {};",
                cypher_string(&decision.decision_id),
                cypher_string(&decision.status),
                cypher_string(&decision.reason),
                decision.confidence,
                cypher_string(&decision.created_at)
            ),
        )?;
        query(
            &conn,
            &format!(
                "MATCH (t:Task {{task_id: {}}}), (d:Decision {{decision_id: {}}}) MERGE (t)-[:HAS_DECISION]->(d);",
                cypher_string(&snapshot.task_id),
                cypher_string(&decision.decision_id)
            ),
        )?;
        for evidence in decision.evidence {
            query(
                &conn,
                &format!(
                    "MERGE (e:Evidence {{evidence_id: {}}}) SET e.kind = {}, e.ref = {}, e.summary = {};",
                    cypher_string(&evidence.evidence_id),
                    cypher_string(&evidence.kind),
                    cypher_string(&evidence.reference),
                    cypher_string(&evidence.summary)
                ),
            )?;
            query(
                &conn,
                &format!(
                    "MATCH (d:Decision {{decision_id: {}}}), (e:Evidence {{evidence_id: {}}}) MERGE (d)-[:DEPENDS_ON]->(e);",
                    cypher_string(&decision.decision_id),
                    cypher_string(&evidence.evidence_id)
                ),
            )?;
        }
    }

    Ok(())
}

fn snapshot_to_provenance(snapshot: TaskGraphSnapshot) -> TaskProvenance {
    let mut nodes = vec![ProvenanceNode {
        id: snapshot.task_id.clone(),
        kind: "Task".to_string(),
        properties: json!({"state": snapshot.task_state}),
    }];
    let mut edges = Vec::new();

    if let Some(workspace) = snapshot.workspace {
        nodes.push(ProvenanceNode {
            id: workspace.workspace_id.clone(),
            kind: "Workspace".to_string(),
            properties: json!({"canonical_path": workspace.canonical_path}),
        });
        edges.push(ProvenanceEdge {
            from: snapshot.task_id.clone(),
            to: workspace.workspace_id,
            kind: "ROUTED_TO".to_string(),
            properties: json!({}),
        });
    }

    if let Some(session) = snapshot.session {
        nodes.push(ProvenanceNode {
            id: session.session_id.clone(),
            kind: "Session".to_string(),
            properties: json!({"client_type": session.client_type}),
        });
        edges.push(ProvenanceEdge {
            from: snapshot.task_id.clone(),
            to: session.session_id.clone(),
            kind: "DISPATCHED_TO".to_string(),
            properties: json!({}),
        });
        if let Some(turn) = snapshot.turn {
            nodes.push(ProvenanceNode {
                id: turn.turn_id.clone(),
                kind: "Turn".to_string(),
                properties: json!({"state": turn.state}),
            });
            edges.push(ProvenanceEdge {
                from: session.session_id,
                to: turn.turn_id,
                kind: "HAS_TURN".to_string(),
                properties: json!({}),
            });
        }
    }

    for decision in snapshot.decisions {
        nodes.push(ProvenanceNode {
            id: decision.decision_id.clone(),
            kind: "Decision".to_string(),
            properties: json!({
                "status": decision.status,
                "reason": decision.reason,
                "confidence": decision.confidence,
                "created_at": decision.created_at
            }),
        });
        edges.push(ProvenanceEdge {
            from: snapshot.task_id.clone(),
            to: decision.decision_id.clone(),
            kind: "HAS_DECISION".to_string(),
            properties: json!({}),
        });
        for evidence in decision.evidence {
            nodes.push(ProvenanceNode {
                id: evidence.evidence_id.clone(),
                kind: "Evidence".to_string(),
                properties: json!({
                    "kind": evidence.kind,
                    "ref": evidence.reference,
                    "summary": evidence.summary
                }),
            });
            edges.push(ProvenanceEdge {
                from: decision.decision_id.clone(),
                to: evidence.evidence_id,
                kind: "DEPENDS_ON".to_string(),
                properties: json!({}),
            });
        }
    }

    TaskProvenance { nodes, edges }
}

#[cfg(feature = "kuzu")]
fn query_task_provenance(db_dir: PathBuf, task_id: &str) -> Result<TaskProvenance> {
    let conn = open_graph_connection(db_dir)?;
    initialize_schema(&conn)?;
    let mut nodes = Vec::new();
    let mut edges = Vec::new();

    let mut task_rows = query(
        &conn,
        &format!(
            "MATCH (t:Task {{task_id: {}}}) RETURN t.task_id, t.state;",
            cypher_string(task_id)
        ),
    )?;
    for row in task_rows.by_ref() {
        let id = string_value(&row[0]);
        nodes.push(ProvenanceNode {
            id,
            kind: "Task".to_string(),
            properties: json!({"state": string_value(&row[1])}),
        });
    }

    let mut decision_rows = query(
        &conn,
        &format!(
            "MATCH (t:Task {{task_id: {}}})-[r:HAS_DECISION]->(d:Decision) RETURN t.task_id, d.decision_id, d.status, d.reason, d.confidence, d.created_at;",
            cypher_string(task_id)
        ),
    )?;
    for row in decision_rows.by_ref() {
        edges.push(ProvenanceEdge {
            from: string_value(&row[0]),
            to: string_value(&row[1]),
            kind: "HAS_DECISION".to_string(),
            properties: json!({}),
        });
        push_unique_node(
            &mut nodes,
            ProvenanceNode {
                id: string_value(&row[1]),
                kind: "Decision".to_string(),
                properties: json!({
                    "status": string_value(&row[2]),
                    "reason": string_value(&row[3]),
                    "confidence": number_value(&row[4]),
                    "created_at": string_value(&row[5])
                }),
            },
        );
    }

    let mut evidence_rows = query(
        &conn,
        &format!(
            "MATCH (t:Task {{task_id: {}}})-[:HAS_DECISION]->(d:Decision)-[r:DEPENDS_ON]->(e:Evidence) RETURN d.decision_id, e.evidence_id, e.kind, e.ref, e.summary;",
            cypher_string(task_id)
        ),
    )?;
    for row in evidence_rows.by_ref() {
        edges.push(ProvenanceEdge {
            from: string_value(&row[0]),
            to: string_value(&row[1]),
            kind: "DEPENDS_ON".to_string(),
            properties: json!({}),
        });
        push_unique_node(
            &mut nodes,
            ProvenanceNode {
                id: string_value(&row[1]),
                kind: "Evidence".to_string(),
                properties: json!({
                    "kind": string_value(&row[2]),
                    "ref": string_value(&row[3]),
                    "summary": string_value(&row[4])
                }),
            },
        );
    }

    let mut workspace_rows = query(
        &conn,
        &format!(
            "MATCH (t:Task {{task_id: {}}})-[r:ROUTED_TO]->(w:Workspace) RETURN t.task_id, w.workspace_id, w.canonical_path;",
            cypher_string(task_id)
        ),
    )?;
    for row in workspace_rows.by_ref() {
        edges.push(ProvenanceEdge {
            from: string_value(&row[0]),
            to: string_value(&row[1]),
            kind: "ROUTED_TO".to_string(),
            properties: json!({}),
        });
        push_unique_node(
            &mut nodes,
            ProvenanceNode {
                id: string_value(&row[1]),
                kind: "Workspace".to_string(),
                properties: json!({"canonical_path": string_value(&row[2])}),
            },
        );
    }

    Ok(TaskProvenance { nodes, edges })
}

#[cfg(feature = "kuzu")]
fn open_graph_connection(db_dir: PathBuf) -> Result<kuzu::Connection<'static>> {
    if let Some(parent) = db_dir
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)?;
    }
    let config = kuzu::SystemConfig::default().enable_multi_writes(true);
    let db = kuzu::Database::new(db_dir, config)
        .map_err(|error| Error::Domain(format!("kuzu database open failed: {error}")))?;
    let db = Box::leak(Box::new(db));
    kuzu::Connection::new(db)
        .map_err(|error| Error::Domain(format!("kuzu connection failed: {error}")))
}

#[cfg(feature = "kuzu")]
fn initialize_schema<'db>(conn: &kuzu::Connection<'db>) -> Result<()> {
    for statement in [
        "CREATE NODE TABLE IF NOT EXISTS Task(task_id STRING, state STRING, PRIMARY KEY(task_id));",
        "CREATE NODE TABLE IF NOT EXISTS Workspace(workspace_id STRING, canonical_path STRING, PRIMARY KEY(workspace_id));",
        "CREATE NODE TABLE IF NOT EXISTS Session(session_id STRING, client_type STRING, PRIMARY KEY(session_id));",
        "CREATE NODE TABLE IF NOT EXISTS Turn(turn_id STRING, state STRING, PRIMARY KEY(turn_id));",
        "CREATE NODE TABLE IF NOT EXISTS Decision(decision_id STRING, status STRING, reason STRING, confidence DOUBLE, created_at STRING, PRIMARY KEY(decision_id));",
        "CREATE NODE TABLE IF NOT EXISTS Evidence(evidence_id STRING, kind STRING, ref STRING, summary STRING, PRIMARY KEY(evidence_id));",
        "CREATE REL TABLE IF NOT EXISTS HAS_DECISION(FROM Task TO Decision);",
        "CREATE REL TABLE IF NOT EXISTS DEPENDS_ON(FROM Decision TO Evidence);",
        "CREATE REL TABLE IF NOT EXISTS ROUTED_TO(FROM Task TO Workspace);",
        "CREATE REL TABLE IF NOT EXISTS DISPATCHED_TO(FROM Task TO Session);",
        "CREATE REL TABLE IF NOT EXISTS HAS_TURN(FROM Session TO Turn);",
    ] {
        query(conn, statement)?;
    }
    Ok(())
}

#[cfg(feature = "kuzu")]
fn query<'db>(conn: &kuzu::Connection<'db>, statement: &str) -> Result<kuzu::QueryResult<'db>> {
    conn.query(statement)
        .map_err(|error| Error::Domain(format!("kuzu query failed: {error}; query: {statement}")))
}

#[cfg(feature = "kuzu")]
fn cypher_string(value: &str) -> String {
    format!("'{}'", value.replace('\\', "\\\\").replace('\'', "\\'"))
}

#[cfg(feature = "kuzu")]
fn string_value(value: &kuzu::Value) -> String {
    match value {
        kuzu::Value::String(value) => value.clone(),
        kuzu::Value::Null(_) => String::new(),
        other => other.to_string(),
    }
}

#[cfg(feature = "kuzu")]
fn number_value(value: &kuzu::Value) -> f64 {
    match value {
        kuzu::Value::Double(value) => *value,
        kuzu::Value::Float(value) => f64::from(*value),
        kuzu::Value::Int64(value) => *value as f64,
        kuzu::Value::Int32(value) => f64::from(*value),
        _ => 0.0,
    }
}

#[cfg(feature = "kuzu")]
fn push_unique_node(nodes: &mut Vec<ProvenanceNode>, node: ProvenanceNode) {
    if !nodes
        .iter()
        .any(|existing| existing.id == node.id && existing.kind == node.kind)
    {
        nodes.push(node);
    }
}
