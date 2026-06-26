#![allow(dead_code)]

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use pontia_application::AppState;
use pontia_config::AppConfig;
use pontia_http as http;
use pontia_storage_sqlite::{connect_sqlite, run_migrations};
use serde_json::{Value, json};
use sqlx::SqlitePool;
use std::{
    path::PathBuf,
    process::{Command, Stdio},
    sync::OnceLock,
};
use tower::ServiceExt;

pub async fn test_state() -> AppState {
    configure_test_runtime_env();
    let db = connect_sqlite("sqlite://:memory:").await.expect("connect");
    run_migrations(&db).await.expect("migrate");
    let config = AppConfig::from_vars(&std::collections::HashMap::new()).expect("default config");
    AppState::builder(db)
        .external_api_token(None)
        .graph(config.graph)
        .workspace_browser(config.workspace_browser)
        .build()
}

fn configure_test_runtime_env() {
    static PONTIA_HOME: OnceLock<PathBuf> = OnceLock::new();
    let pontia_home = PONTIA_HOME.get_or_init(|| {
        let dir = tempfile::tempdir().expect("agent-tools pontia home tempdir");
        dir.keep()
    });
    unsafe {
        std::env::set_var("PONTIA_HOME", pontia_home);
        std::env::set_var(
            "PONTIA_PI_TUI_COMMAND",
            "cat >> \"$PONTIA_WORKSPACE/pi-tui-input.log\"",
        );
    }
}

pub async fn post_tool(state: AppState, tool: &str, body: Value) -> (StatusCode, Value) {
    let response = http::router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/internal/v1/agent-tools/{tool}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .expect("request"),
        )
        .await
        .expect("response");

    let status = response.status();
    let body = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    let json = serde_json::from_slice(&body).expect("json body");
    (status, json)
}

pub async fn insert_task(pool: &SqlitePool, task_id: &str) {
    sqlx::query("INSERT INTO tasks (task_id, state, input) VALUES (?, 'running', 'test task')")
        .bind(task_id)
        .execute(pool)
        .await
        .expect("insert task");
}

pub async fn cleanup_runtime_sessions(pool: &SqlitePool) {
    let targets: Vec<String> = sqlx::query_scalar(
        "SELECT json_extract(metadata, '$.tmux.session_name') FROM runtime_bindings WHERE json_extract(metadata, '$.tmux.session_name') IS NOT NULL",
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    for target in targets {
        let _ = Command::new("tmux")
            .args(["kill-session", "-t", &target])
            .stderr(Stdio::null())
            .status();
    }
}

pub async fn insert_dag_session(
    pool: &SqlitePool,
    session_id: &str,
    turn_id: &str,
    runtime_instance_id: &str,
    metadata: Value,
) {
    insert_dag_session_with_client(
        pool,
        session_id,
        turn_id,
        runtime_instance_id,
        "pi",
        metadata,
    )
    .await;
}

pub async fn insert_dag_session_with_client(
    pool: &SqlitePool,
    session_id: &str,
    turn_id: &str,
    runtime_instance_id: &str,
    client_type: &str,
    metadata: Value,
) {
    sqlx::query(
        r#"INSERT INTO sessions (session_id, client_type, state, current_turn_id, metadata)
           VALUES (?, ?, 'busy', ?, ?)"#,
    )
    .bind(session_id)
    .bind(client_type)
    .bind(turn_id)
    .bind(metadata.to_string())
    .execute(pool)
    .await
    .expect("insert session");
    sqlx::query(
        "INSERT INTO turns (turn_id, session_id, state, metadata) VALUES (?, ?, 'running', ?)",
    )
    .bind(turn_id)
    .bind(session_id)
    .bind(metadata.to_string())
    .execute(pool)
    .await
    .expect("insert turn");
    sqlx::query(
        "INSERT INTO runtime_bindings (session_id, runtime_kind, runtime_instance_id, metadata) VALUES (?, 'tmux', ?, ?)",
    )
    .bind(session_id)
    .bind(runtime_instance_id)
    .bind(json!({"runtime_instance_id": runtime_instance_id}).to_string())
    .execute(pool)
    .await
    .expect("insert runtime binding");
}

pub async fn insert_work_item(
    pool: &SqlitePool,
    task_id: &str,
    work_item_id: &str,
    title: &str,
    state: &str,
    acceptance_criteria: Value,
) {
    sqlx::query(
        r#"INSERT INTO graph_work_items (
                work_item_id, task_id, title, description, kind, action,
                execution_profile_id, acceptance_criteria
           ) VALUES (?, ?, ?, 'Do the current work', 'implementation', 'agent_turn', 'implementer', ?)"#,
    )
    .bind(work_item_id)
    .bind(task_id)
    .bind(title)
    .bind(acceptance_criteria.to_string())
    .execute(pool)
    .await
    .expect("insert work item");
    sqlx::query(
        r#"INSERT INTO work_item_runtime_projection (
                work_item_id, task_id, current_state, current_attempt, priority
           ) VALUES (?, ?, ?, 0, 0)"#,
    )
    .bind(work_item_id)
    .bind(task_id)
    .bind(state)
    .execute(pool)
    .await
    .expect("insert work item runtime");
}

pub async fn insert_execution_run(
    pool: &SqlitePool,
    task_id: &str,
    work_item_id: &str,
    run_id: &str,
    session_id: &str,
    turn_id: &str,
) {
    insert_work_item(
        pool,
        task_id,
        work_item_id,
        "Do work",
        "running",
        json!(["done means done"]),
    )
    .await;
    sqlx::query(
        r#"INSERT INTO work_item_runs (
                run_id, work_item_id, task_id, attempt, state, session_id, turn_id,
                client_type, execution_profile_id, rendered_prompt_ref
           ) VALUES (?, ?, ?, 1, 'running', ?, ?, 'pi', 'implementer', 'inline')"#,
    )
    .bind(run_id)
    .bind(work_item_id)
    .bind(task_id)
    .bind(session_id)
    .bind(turn_id)
    .execute(pool)
    .await
    .expect("insert run");
    sqlx::query(
        r#"UPDATE work_item_runtime_projection
           SET current_run_id = ?, current_attempt = 1, session_id = ?, turn_id = ?
           WHERE work_item_id = ?"#,
    )
    .bind(run_id)
    .bind(session_id)
    .bind(turn_id)
    .bind(work_item_id)
    .execute(pool)
    .await
    .expect("update work item runtime");
}

pub async fn insert_edge(pool: &SqlitePool, task_id: &str, from: &str, to: &str) {
    sqlx::query(
        "INSERT INTO graph_work_item_edges (edge_id, task_id, from_work_item_id, to_work_item_id, edge_type) VALUES (?, ?, ?, ?, 'depends_on')",
    )
    .bind(format!("edge_{from}_{to}"))
    .bind(task_id)
    .bind(from)
    .bind(to)
    .execute(pool)
    .await
    .expect("insert edge");
}

pub async fn insert_signal(
    pool: &SqlitePool,
    signal_id: &str,
    task_id: &str,
    work_item_id: Option<&str>,
    run_id: Option<&str>,
    state: &str,
) {
    sqlx::query(
        r#"INSERT INTO dag_signals (
                signal_id, task_id, work_item_id, run_id, kind, summary, severity, state
           ) VALUES (?, ?, ?, ?, 'needs_input', 'Need input', 'medium', ?)"#,
    )
    .bind(signal_id)
    .bind(task_id)
    .bind(work_item_id)
    .bind(run_id)
    .bind(state)
    .execute(pool)
    .await
    .expect("insert signal");
}

pub async fn insert_proposal(pool: &SqlitePool, proposal_id: &str, task_id: &str, state: &str) {
    let session_id = format!("sess_{proposal_id}");
    let turn_id = format!("turn_{proposal_id}");
    let metadata = json!({
        "dag_managed": true,
        "dag_planning_role": "planner",
        "task_id": task_id
    });
    sqlx::query(
        r#"INSERT OR IGNORE INTO sessions (session_id, client_type, state, metadata)
           VALUES (?, 'pi', 'busy', ?)"#,
    )
    .bind(&session_id)
    .bind(metadata.to_string())
    .execute(pool)
    .await
    .expect("insert proposal session");
    sqlx::query(
        r#"INSERT OR IGNORE INTO turns (turn_id, session_id, state, metadata)
           VALUES (?, ?, 'completed', ?)"#,
    )
    .bind(&turn_id)
    .bind(&session_id)
    .bind(metadata.to_string())
    .execute(pool)
    .await
    .expect("insert proposal turn");
    sqlx::query(
        r#"INSERT INTO dag_proposals (
                proposal_id, task_id, mode, state, summary, proposal_json, validation_json,
                created_by_session_id, created_by_turn_id
           ) VALUES (?, ?, 'initial_dag', ?, 'Initial plan', '{"work_items":[]}', '{}', ?, ?)"#,
    )
    .bind(proposal_id)
    .bind(task_id)
    .bind(state)
    .bind(session_id)
    .bind(turn_id)
    .execute(pool)
    .await
    .expect("insert proposal");
}

pub fn valid_initial_dag_input() -> Value {
    json!({
        "mode": "initial_dag",
        "summary": "Implement the task",
        "dag": {
            "work_items": [{
                "temp_id": "impl",
                "title": "Implement",
                "description": "Do the implementation",
                "kind": "implementation",
                "action": "agent_turn",
                "execution_profile_id": "implementer",
                "acceptance_criteria": ["done"]
            }],
            "edges": []
        },
        "assumptions": [],
        "risks": []
    })
}

pub fn valid_patch_input() -> Value {
    json!({
        "mode": "patch",
        "summary": "Add follow-up",
        "patch": {
            "operations": [{
                "op": "add_work_item",
                "work_item": {
                    "temp_id": "followup",
                    "title": "Follow up",
                    "description": "Do follow-up work",
                    "kind": "implementation",
                    "action": "agent_turn",
                    "execution_profile_id": "implementer",
                    "acceptance_criteria": ["follow-up done"]
                }
            }]
        }
    })
}
