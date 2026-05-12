use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use llmparty::{
    application::AppState,
    config::AppConfig,
    storage::sqlite::{connect_sqlite, run_migrations},
    transport::http,
};
use serde_json::{Value, json};
use sqlx::SqlitePool;
use tower::ServiceExt;

async fn test_state() -> AppState {
    let db = connect_sqlite("sqlite://:memory:").await.expect("connect");
    run_migrations(&db).await.expect("migrate");
    let config = AppConfig::from_vars(&std::collections::HashMap::new()).expect("default config");
    AppState {
        db,
        external_api_token: None,
        planner: config.planner,
        graph: config.graph,
        workspace_browser: config.workspace_browser,
    }
}

async fn post_tool(state: AppState, tool: &str, body: Value) -> (StatusCode, Value) {
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

async fn insert_task(pool: &SqlitePool, task_id: &str) {
    sqlx::query("INSERT INTO tasks (task_id, state, input) VALUES (?, 'running', 'test task')")
        .bind(task_id)
        .execute(pool)
        .await
        .expect("insert task");
}

async fn insert_dag_session(
    pool: &SqlitePool,
    session_id: &str,
    turn_id: &str,
    runtime_instance_id: &str,
    metadata: Value,
) {
    sqlx::query(
        r#"INSERT INTO sessions (session_id, client_type, state, current_turn_id, metadata)
           VALUES (?, 'pi', 'busy', ?, ?)"#,
    )
    .bind(session_id)
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
        "INSERT INTO runtime_bindings (session_id, runtime_kind, runtime_ref, metadata) VALUES (?, 'tmux', ?, ?)",
    )
    .bind(session_id)
    .bind(format!("rtref_{session_id}"))
    .bind(json!({"runtime_instance_id": runtime_instance_id}).to_string())
    .execute(pool)
    .await
    .expect("insert runtime binding");
}

async fn insert_execution_run(
    pool: &SqlitePool,
    task_id: &str,
    work_item_id: &str,
    run_id: &str,
    session_id: &str,
    turn_id: &str,
) {
    sqlx::query(
        r#"INSERT INTO work_items (
                work_item_id, task_id, title, description, kind, action,
                execution_profile_id, acceptance_criteria
           ) VALUES (?, ?, 'Do work', 'Do the current work', 'implementation', 'agent_turn', 'implementer', '[]')"#,
    )
    .bind(work_item_id)
    .bind(task_id)
    .execute(pool)
    .await
    .expect("insert work item");
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
}

#[tokio::test]
async fn rejects_unknown_tool_and_invalid_requests() {
    let state = test_state().await;

    let (status, body) = post_tool(
        state.clone(),
        "noSuchTool",
        json!({
            "session_id": "sess_missing",
            "turn_id": "turn_missing",
            "runtime_instance_id": "rt_missing",
            "input": {}
        }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "not_found");

    let (status, body) = post_tool(state, "getContext", json!({"input": {}})).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "invalid_request");
}

#[tokio::test]
async fn authorizes_planning_context_from_session_turn_and_runtime_binding() {
    let state = test_state().await;
    insert_task(&state.db, "task_plan").await;
    insert_dag_session(
        &state.db,
        "sess_plan",
        "turn_plan",
        "rt_plan",
        json!({
            "dag_managed": true,
            "dag_planning_role": "planner",
            "task_id": "task_plan"
        }),
    )
    .await;

    let (status, body) = post_tool(
        state,
        "getContext",
        json!({
            "session_id": "sess_plan",
            "turn_id": "turn_plan",
            "runtime_instance_id": "rt_plan",
            "input": {"task_id": "task_other", "run_id": "run_other"}
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["ok"], true);
    assert_eq!(body["tool"], "getContext");
    let context = &body["result"]["context"];
    assert_eq!(context["session_id"], "sess_plan");
    assert_eq!(context["turn_id"], "turn_plan");
    assert_eq!(context["client_type"], "pi");
    assert_eq!(context["runtime_instance_id"], "rt_plan");
    assert_eq!(context["task_id"], "task_plan");
    assert_eq!(context["mode"]["type"], "planning");
    assert_eq!(context["mode"]["role"], "planner");
}

#[tokio::test]
async fn authorizes_execution_context_from_current_work_item_run_not_request_input() {
    let state = test_state().await;
    insert_task(&state.db, "task_exec").await;
    insert_task(&state.db, "task_other").await;
    insert_dag_session(
        &state.db,
        "sess_exec",
        "turn_exec",
        "rt_exec",
        json!({"dag_managed": true}),
    )
    .await;
    insert_execution_run(
        &state.db,
        "task_exec",
        "wi_exec",
        "run_exec",
        "sess_exec",
        "turn_exec",
    )
    .await;

    let (status, body) = post_tool(
        state,
        "submitResult",
        json!({
            "session_id": "sess_exec",
            "turn_id": "turn_exec",
            "runtime_instance_id": "rt_exec",
            "input": {"task_id": "task_other", "run_id": "run_other", "work_item_id": "wi_other"}
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let context = &body["result"]["context"];
    assert_eq!(context["task_id"], "task_exec");
    assert_eq!(context["mode"]["type"], "execution");
    assert_eq!(context["mode"]["run_id"], "run_exec");
    assert_eq!(context["mode"]["work_item_id"], "wi_exec");
}

#[tokio::test]
async fn rejects_runtime_mismatch_and_non_dag_managed_turns() {
    let state = test_state().await;
    insert_task(&state.db, "task_plan").await;
    insert_dag_session(
        &state.db,
        "sess_plan",
        "turn_plan",
        "rt_expected",
        json!({
            "dag_managed": true,
            "dag_planning_role": "replanner",
            "task_id": "task_plan"
        }),
    )
    .await;

    let (status, body) = post_tool(
        state.clone(),
        "raiseSignal",
        json!({
            "session_id": "sess_plan",
            "turn_id": "turn_plan",
            "runtime_instance_id": "rt_wrong",
            "input": {}
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "state_conflict");

    insert_dag_session(
        &state.db,
        "sess_plain",
        "turn_plain",
        "rt_plain",
        json!({"task_id": "task_plan"}),
    )
    .await;
    let (status, body) = post_tool(
        state,
        "getContext",
        json!({
            "session_id": "sess_plain",
            "turn_id": "turn_plain",
            "runtime_instance_id": "rt_plain",
            "input": {}
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("DAG-managed")
    );
}
