use std::sync::{Arc, Mutex};

use pontia_application::CreateSessionRequest;
use pontia_storage_sqlite::repositories::workflows::SqliteWorkflowRepository;
use pontia_workflow::SessionCreator;
use serde_json::{Value, json};

use crate::fixture::seed_linear_workflow;

#[derive(Clone)]
pub(super) struct PersistingSessionCreator {
    pool: sqlx::SqlitePool,
    pub(super) requests: Arc<Mutex<Vec<CreateSessionRequest>>>,
    session_id: &'static str,
    turn_id: &'static str,
    runtime_id: &'static str,
}

impl PersistingSessionCreator {
    pub(super) fn new(pool: sqlx::SqlitePool) -> Self {
        Self::with_identity(
            pool,
            "sess_replanner",
            "turn_replanner",
            "runtime_replanner",
        )
    }

    pub(super) fn with_identity(
        pool: sqlx::SqlitePool,
        session_id: &'static str,
        turn_id: &'static str,
        runtime_id: &'static str,
    ) -> Self {
        Self {
            pool,
            requests: Arc::new(Mutex::new(Vec::new())),
            session_id,
            turn_id,
            runtime_id,
        }
    }
}

impl SessionCreator for PersistingSessionCreator {
    async fn find_session_by_creation_token(
        &self,
        metadata_key: &str,
        token: &str,
    ) -> pontia_workflow::Result<Option<String>> {
        let path = format!("$.{metadata_key}");
        Ok(sqlx::query_scalar(
            "SELECT session_id FROM sessions WHERE json_extract(metadata, ?) = ?",
        )
        .bind(path)
        .bind(token)
        .fetch_optional(&self.pool)
        .await
        .map_err(pontia_core::Error::from)?)
    }

    async fn create_session(
        &self,
        request: CreateSessionRequest,
    ) -> pontia_workflow::Result<String> {
        self.requests
            .lock()
            .expect("requests")
            .push(request.clone());
        seed_replanner_session(
            &self.pool,
            self.session_id,
            self.turn_id,
            self.runtime_id,
            &request.metadata,
        )
        .await;
        Ok(self.session_id.into())
    }
}

pub(super) fn write_problem_report(
    pontia_home: &std::path::Path,
    workflow_id: &str,
    content: &str,
) {
    let node_dir = pontia_home
        .join("workflows")
        .join(workflow_id)
        .join("nodes")
        .join(format!("{workflow_id}_root"));
    std::fs::create_dir_all(&node_dir).unwrap();
    std::fs::write(node_dir.join("problem-report.md"), content).unwrap();
}

pub(super) fn write_patch_file(
    pontia_home: &std::path::Path,
    workflow_id: &str,
    patch_id: &str,
    name: &str,
    content: &str,
) {
    std::fs::write(
        pontia_home
            .join("workflows")
            .join(workflow_id)
            .join("patches")
            .join(patch_id)
            .join(name),
        content,
    )
    .unwrap();
}

pub(super) async fn seed_requester(
    pool: &sqlx::SqlitePool,
    repository: &SqliteWorkflowRepository,
    pontia_home: &std::path::Path,
    workflow_id: &str,
    with_child: bool,
) {
    seed_linear_workflow(repository, workflow_id, "[]", with_child).await;
    repository
        .start_workflow(workflow_id, "evt_started")
        .await
        .unwrap();
    sqlx::query("INSERT INTO sessions (session_id, client_type, state, current_turn_id) VALUES ('sess_requester', 'pi', 'busy', 'turn_requester')")
        .execute(pool).await.unwrap();
    repository
        .bind_node_session(&format!("{workflow_id}_root"), "sess_requester")
        .await
        .unwrap();
    sqlx::query("INSERT INTO turns (turn_id, session_id, state, topology_status) VALUES ('turn_requester', 'sess_requester', 'running', 'root')")
        .execute(pool).await.unwrap();
    sqlx::query("INSERT INTO runtime_bindings (session_id, runtime_kind, runtime_instance_id, binding_state) VALUES ('sess_requester', 'pi_tui', 'runtime_requester', 'confirmed')")
        .execute(pool).await.unwrap();
    insert_fact(
        pool,
        "evt_requester_started",
        "sess_requester",
        "turn_requester",
        "turn.started",
        "runtime_requester",
    )
    .await;
    let workflow_dir = pontia_home.join("workflows").join(workflow_id);
    std::fs::create_dir_all(&workflow_dir).unwrap();
    let root_id = format!("{workflow_id}_root");
    let mut definition = format!(
        r#"workflow_id = "{workflow_id}"
revision = 1
title = "Convergence workflow"
cwd = "/workspace/project"

[[nodes]]
id = "{root_id}"
type = "agent"
phase = "Test Phase"
title = "Root"
instructions = "Produce the root output."
inputs = []
output = "root.md"
"#
    );
    if with_child {
        definition.push_str(&format!(
            r#"
[[nodes]]
id = "{workflow_id}_child"
type = "agent"
phase = "Test Phase"
title = "Child"
instructions = "Produce the child output."
inputs = ["root.md"]
output = "child.md"
"#
        ));
    }
    std::fs::write(workflow_dir.join("workflow.toml"), definition).unwrap();
}

pub(super) async fn seed_replanner_session(
    pool: &sqlx::SqlitePool,
    session_id: &str,
    turn_id: &str,
    runtime_id: &str,
    metadata: &Value,
) {
    sqlx::query("INSERT INTO sessions (session_id, client_type, state, current_turn_id, metadata) VALUES (?, 'pi', 'busy', ?, ?)")
        .bind(session_id).bind(turn_id).bind(metadata.to_string()).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO turns (turn_id, session_id, state, topology_status) VALUES (?, ?, 'running', 'root')")
        .bind(turn_id).bind(session_id).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO runtime_bindings (session_id, runtime_kind, runtime_instance_id, binding_state) VALUES (?, 'pi_tui', ?, 'confirmed')")
        .bind(session_id).bind(runtime_id).execute(pool).await.unwrap();
    insert_fact(
        pool,
        &format!("evt_{turn_id}_started"),
        session_id,
        turn_id,
        "turn.started",
        runtime_id,
    )
    .await;
}

pub(super) async fn insert_fact(
    pool: &sqlx::SqlitePool,
    event_id: &str,
    session_id: &str,
    turn_id: &str,
    event_type: &str,
    runtime_id: &str,
) {
    sqlx::query(
        r#"INSERT INTO events
           (event_id, session_id, turn_id, source, client_type, event_type, occurred_at, payload)
           VALUES (?, ?, ?, 'agent_adapter', 'pi', ?, '2026-08-01T00:00:00Z', ?)"#,
    )
    .bind(event_id)
    .bind(session_id)
    .bind(turn_id)
    .bind(event_type)
    .bind(if event_type == "turn.started" {
        json!({ "runtime_instance_id": runtime_id }).to_string()
    } else {
        "{}".into()
    })
    .execute(pool)
    .await
    .unwrap();
}
