use pontia::storage::sqlite::{
    connect_sqlite, repositories::dag::SqliteDagRepository, run_migrations,
};
use serde_json::json;

async fn test_pool() -> sqlx::SqlitePool {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("sqlite_dag_repository.db");
    let _kept_dir = dir.keep();
    let database_url = format!("sqlite://{}", db_path.display());
    let pool = connect_sqlite(&database_url).await.expect("connect");
    run_migrations(&pool).await.expect("migrate");
    pool
}

async fn insert_task(pool: &sqlx::SqlitePool, task_id: &str) {
    sqlx::query("INSERT INTO tasks (task_id, state, input) VALUES (?, 'running', 'dag repo')")
        .bind(task_id)
        .execute(pool)
        .await
        .expect("insert task");
}

#[tokio::test]
async fn sqlite_dag_repository_lists_work_items_edges_runtime_runs_and_signals() {
    let pool = test_pool().await;
    insert_task(&pool, "task_dag_repo").await;

    sqlx::query(
        r#"INSERT INTO work_items (
              work_item_id, task_id, title, description, kind, action,
              execution_profile_id, execution_profile_version, priority, optional,
              parallelizable, acceptance_criteria, metadata, created_at, updated_at
           ) VALUES
              ('wi_low', 'task_dag_repo', 'Low', 'Low priority', 'implementation', 'agent_turn',
               'default', '1', 1, 0, 1, ?, ?, '2026-06-15T12:00:00Z', '2026-06-15T12:00:00Z'),
              ('wi_high', 'task_dag_repo', 'High', 'High priority', 'implementation', 'agent_turn',
               'default', '1', 10, 1, 0, ?, ?, '2026-06-15T12:00:01Z', '2026-06-15T12:00:01Z')"#,
    )
    .bind(json!(["low done"]).to_string())
    .bind(json!({"rank": 1}).to_string())
    .bind(json!(["high done"]).to_string())
    .bind(json!({"rank": 10}).to_string())
    .execute(&pool)
    .await
    .expect("insert work items");

    sqlx::query(
        r#"INSERT INTO work_item_edges (edge_id, task_id, from_work_item_id, to_work_item_id, edge_type, created_at)
           VALUES ('edge_2', 'task_dag_repo', 'wi_high', 'wi_low', 'depends_on', '2026-06-15T12:00:01Z'),
                  ('edge_1', 'task_dag_repo', 'wi_low', 'wi_high', 'depends_on', '2026-06-15T12:00:00Z')"#,
    )
    .execute(&pool)
    .await
    .expect("insert edges");

    sqlx::query(
        r#"INSERT INTO work_item_runs (
              run_id, work_item_id, task_id, attempt, state, execution_profile_id,
              execution_profile_version, failure, created_at, updated_at
           ) VALUES
              ('run_b', 'wi_high', 'task_dag_repo', 1, 'completed', 'default', '1', ?,
               '2026-06-15T12:00:00Z', '2026-06-15T12:00:00Z'),
              ('run_a', 'wi_low', 'task_dag_repo', 1, 'failed', 'default', '1', ?,
               '2026-06-15T12:00:00Z', '2026-06-15T12:00:01Z')"#,
    )
    .bind(json!({"ignored": true}).to_string())
    .bind(json!({"message": "boom"}).to_string())
    .execute(&pool)
    .await
    .expect("insert runs");

    sqlx::query(
        r#"INSERT INTO work_item_runtime_projection (
              work_item_id, task_id, current_run_id, current_state, current_attempt,
              retry_count, max_retries, priority, optional, parallelizable, updated_at
           ) VALUES
              ('wi_high', 'task_dag_repo', 'run_b', 'completed', 1, 0, 0, 10, 1, 0, '2026-06-15T12:00:01Z'),
              ('wi_low', 'task_dag_repo', 'run_a', 'failed', 1, 1, 3, 1, 0, 1, '2026-06-15T12:00:02Z')"#,
    )
    .execute(&pool)
    .await
    .expect("insert runtime projection");

    sqlx::query(
        r#"INSERT INTO dag_signals (
              signal_id, task_id, work_item_id, run_id, source, kind, summary,
              severity, related_refs, state, created_at, updated_at
           ) VALUES
              ('sig_b', 'task_dag_repo', 'wi_high', 'run_b', 'agent', 'notice', 'B', 'low', ?, 'resolved', '2026-06-15T12:00:00Z', '2026-06-15T12:00:00Z'),
              ('sig_a', 'task_dag_repo', 'wi_low', 'run_a', 'human', 'blocked', 'A', 'high', ?, 'open', '2026-06-15T12:00:00Z', '2026-06-15T12:00:01Z')"#,
    )
    .bind(json!([{"id":"b"}]).to_string())
    .bind(json!([{"id":"a"}]).to_string())
    .execute(&pool)
    .await
    .expect("insert signals");

    let repository = SqliteDagRepository::new(pool);

    let work_items = repository
        .list_work_items("task_dag_repo")
        .await
        .expect("list work items");
    assert_eq!(work_items[0].work_item_id, "wi_high");
    assert!(work_items[0].optional);
    assert!(!work_items[0].parallelizable);
    assert_eq!(
        work_items[0].acceptance_criteria,
        json!(["high done"]).to_string()
    );

    let edges = repository
        .list_work_item_edges("task_dag_repo")
        .await
        .expect("list edges");
    assert_eq!(
        edges
            .iter()
            .map(|edge| edge.edge_id.as_str())
            .collect::<Vec<_>>(),
        vec!["edge_1", "edge_2"]
    );

    let runs = repository
        .list_work_item_runs("task_dag_repo")
        .await
        .expect("list runs");
    assert_eq!(
        runs.iter()
            .map(|run| run.run_id.as_str())
            .collect::<Vec<_>>(),
        vec!["run_a", "run_b"]
    );
    assert_eq!(runs[0].failure.as_deref(), Some(r#"{"message":"boom"}"#));

    let runtime = repository
        .list_runtime_projection("task_dag_repo")
        .await
        .expect("list runtime");
    assert_eq!(runtime.len(), 2);
    assert_eq!(runtime[0].current_state, "completed");

    let signals = repository
        .list_dag_signals("task_dag_repo")
        .await
        .expect("list signals");
    assert_eq!(
        signals
            .iter()
            .map(|signal| signal.signal_id.as_str())
            .collect::<Vec<_>>(),
        vec!["sig_a", "sig_b"]
    );
    assert_eq!(signals[0].related_refs, json!([{"id":"a"}]).to_string());

    let signal = repository
        .get_dag_signal("sig_a")
        .await
        .expect("get signal")
        .expect("signal exists");
    assert_eq!(signal.summary, "A");
    assert_eq!(
        repository
            .count_open_signals("task_dag_repo")
            .await
            .expect("open signals"),
        1
    );
    assert_eq!(
        repository
            .count_work_item_runs("task_dag_repo")
            .await
            .expect("run count"),
        2
    );
}

#[tokio::test]
async fn sqlite_dag_repository_lists_proposals_with_existing_filters_and_order() {
    let pool = test_pool().await;
    insert_task(&pool, "task_proposals").await;
    sqlx::query(
        r#"INSERT INTO sessions (session_id, client_type, state)
           VALUES ('sess_planner', 'generic', 'idle')"#,
    )
    .execute(&pool)
    .await
    .expect("insert session");
    sqlx::query(
        r#"INSERT INTO turns (turn_id, session_id, state)
           VALUES ('turn_planner', 'sess_planner', 'completed')"#,
    )
    .execute(&pool)
    .await
    .expect("insert turn");

    sqlx::query(
        r#"INSERT INTO dag_proposals (
              proposal_id, task_id, mode, state, summary, proposal_json, validation_json,
              created_by_session_id, created_by_turn_id, revision, created_at, updated_at
           ) VALUES
              ('prop_old', 'task_proposals', 'initial_dag', 'applied', 'Old', ?, ?, 'sess_planner', 'turn_planner', 1, '2026-06-15T12:00:00Z', '2026-06-15T12:00:00Z'),
              ('prop_rejected', 'task_proposals', 'patch', 'rejected', 'Rejected', ?, ?, 'sess_planner', 'turn_planner', 2, '2026-06-15T12:00:01Z', '2026-06-15T12:00:01Z'),
              ('prop_new', 'task_proposals', 'patch', 'proposed', 'New', ?, ?, 'sess_planner', 'turn_planner', 3, '2026-06-15T12:00:02Z', '2026-06-15T12:00:02Z')"#,
    )
    .bind(json!({"old": true}).to_string())
    .bind(json!({}).to_string())
    .bind(json!({"rejected": true}).to_string())
    .bind(json!({"valid": false}).to_string())
    .bind(json!({"new": true}).to_string())
    .bind(json!({"valid": true}).to_string())
    .execute(&pool)
    .await
    .expect("insert proposals");

    let repository = SqliteDagRepository::new(pool);
    let all = repository
        .list_task_dag_proposals("task_proposals")
        .await
        .expect("list proposals");
    assert_eq!(
        all.iter()
            .map(|row| row.proposal_id.as_str())
            .collect::<Vec<_>>(),
        vec!["prop_new", "prop_rejected", "prop_old"]
    );

    let relevant = repository
        .list_relevant_dag_proposals("task_proposals")
        .await
        .expect("list relevant proposals");
    assert_eq!(
        relevant
            .iter()
            .map(|row| row.proposal_id.as_str())
            .collect::<Vec<_>>(),
        vec!["prop_new", "prop_rejected"]
    );
    assert_eq!(relevant[0].proposal_json, json!({"new": true}).to_string());
}
