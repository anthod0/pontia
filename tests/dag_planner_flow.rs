use llmparty::{
    application::{
        DagPlanningService, DagService, SqliteDagGraphStore, SubmitPlanPayload, WorkItemDraft,
    },
    ids::new_task_id,
    storage::sqlite::{connect_sqlite, run_migrations},
};
use serde_json::{Value, json};
use sqlx::{Row, SqlitePool};
use std::process::Command;

async fn test_pool() -> SqlitePool {
    let db = connect_sqlite("sqlite://:memory:").await.expect("connect");
    run_migrations(&db).await.expect("migrate");
    db
}

async fn cleanup_runtime_sessions(pool: &SqlitePool) {
    let refs: Vec<String> = sqlx::query_scalar("SELECT runtime_ref FROM runtime_bindings")
        .fetch_all(pool)
        .await
        .unwrap_or_default();
    for runtime_ref in refs {
        let _ = Command::new("tmux")
            .args(["kill-session", "-t", &runtime_ref])
            .stderr(std::process::Stdio::null())
            .status();
    }
}

async fn insert_created_task(pool: &SqlitePool) -> String {
    let task_id = new_task_id().to_string();
    sqlx::query(
        "INSERT INTO tasks (task_id, state, input) VALUES (?, 'created', 'build the feature')",
    )
    .bind(&task_id)
    .execute(pool)
    .await
    .expect("insert task");
    task_id
}

fn worker_draft(temp_id: &str) -> WorkItemDraft {
    WorkItemDraft {
        temp_id: Some(temp_id.to_string()),
        title: format!("{temp_id} title"),
        description: format!("{temp_id} description"),
        kind: "implementation".to_string(),
        action: "agent_turn".to_string(),
        execution_profile_id: "implementer".to_string(),
        execution_profile_version: None,
        priority: 0,
        optional: false,
        parallelizable: true,
        acceptance_criteria: vec!["done".to_string()],
        metadata: json!({}),
    }
}

fn initial_plan(work_items: Vec<WorkItemDraft>) -> SubmitPlanPayload {
    SubmitPlanPayload {
        mode: "initial_dag".to_string(),
        summary: "initial plan".to_string(),
        work_items,
        edges: vec![],
        assumptions: vec![],
        risks: vec![],
    }
}

#[tokio::test]
async fn planner_output_applies_initial_dag_and_starts_scheduler_without_routing_planner() {
    let pool = test_pool().await;
    let task_id = insert_created_task(&pool).await;
    let service = DagPlanningService::new(pool.clone());

    let planner_turn = service
        .start_initial_planning(&task_id)
        .await
        .expect("start planner");
    assert_eq!(planner_turn.task_id, task_id);
    assert_eq!(planner_turn.profile_id, "planner");
    assert!(!planner_turn.session_id.is_empty());
    assert!(!planner_turn.turn_id.is_empty());

    let output = json!({
        "mode": "initial_dag",
        "summary": "Plan for build the feature",
        "dag": {
            "work_items": [{
                "temp_id": "impl",
                "title": "Implement feature",
                "description": "Make the requested change",
                "kind": "implementation",
                "action": "agent_turn",
                "execution_profile_id": "implementer",
                "acceptance_criteria": ["Feature is implemented"]
            }],
            "edges": []
        },
        "assumptions": [],
        "risks": []
    });

    let outcome = service
        .submit_planner_output(&task_id, &planner_turn.session_id, output.to_string())
        .await
        .expect("submit planner output");

    assert_eq!(outcome.proposal.mode, "initial_dag");
    assert_eq!(outcome.proposal.state, "applied");
    assert_eq!(outcome.scheduler.dispatched_runs.len(), 1);

    let task_row = sqlx::query("SELECT state, routing_state, turn_id FROM tasks WHERE task_id = ?")
        .bind(&task_id)
        .fetch_one(&pool)
        .await
        .expect("task row");
    assert_eq!(task_row.get::<String, _>("state"), "running");
    assert_eq!(task_row.get::<String, _>("routing_state"), "pending");
    assert!(
        task_row
            .try_get::<Option<String>, _>("turn_id")
            .unwrap()
            .is_none()
    );

    let proposal_state: String =
        sqlx::query_scalar("SELECT state FROM dag_proposals WHERE proposal_id = ?")
            .bind(&outcome.proposal.proposal_id)
            .fetch_one(&pool)
            .await
            .expect("proposal state");
    assert_eq!(proposal_state, "applied");

    let planner_session_metadata: String =
        sqlx::query_scalar("SELECT metadata FROM sessions WHERE session_id = ?")
            .bind(&planner_turn.session_id)
            .fetch_one(&pool)
            .await
            .expect("planner session metadata");
    let metadata: Value = serde_json::from_str(&planner_session_metadata).expect("metadata json");
    assert_eq!(metadata["dag_managed"], true);
    assert_eq!(metadata["dag_planning_role"], "planner");
    assert_eq!(metadata["task_id"], task_id);
    let planner_session_state: String =
        sqlx::query_scalar("SELECT state FROM sessions WHERE session_id = ?")
            .bind(&planner_turn.session_id)
            .fetch_one(&pool)
            .await
            .expect("planner session state");
    assert_eq!(planner_session_state, "exited");

    cleanup_runtime_sessions(&pool).await;
}

#[tokio::test]
async fn replanner_output_applies_patch_and_resumes_scheduler() {
    let pool = test_pool().await;
    let task_id = insert_created_task(&pool).await;
    DagService::new(pool.clone())
        .apply_initial_dag(&task_id, &initial_plan(vec![worker_draft("done")]))
        .await
        .expect("apply initial dag");
    sqlx::query("UPDATE tasks SET state = 'running' WHERE task_id = ?")
        .bind(&task_id)
        .execute(&pool)
        .await
        .expect("mark running");
    sqlx::query(
        "UPDATE work_item_runtime_projection SET current_state = 'completed' WHERE task_id = ?",
    )
    .bind(&task_id)
    .execute(&pool)
    .await
    .expect("mark existing completed");
    sqlx::query(
        r#"INSERT INTO dag_signals (signal_id, task_id, kind, summary, severity)
           VALUES ('sig_replan', ?, 'replan_requested', 'Need a follow-up work item', 'medium')"#,
    )
    .bind(&task_id)
    .execute(&pool)
    .await
    .expect("insert signal");
    sqlx::query(
        r#"INSERT INTO dag_signals (signal_id, task_id, kind, summary, severity)
           VALUES ('sig_unrelated', ?, 'risk', 'Keep this signal open', 'low')"#,
    )
    .bind(&task_id)
    .execute(&pool)
    .await
    .expect("insert unrelated signal");

    let service = DagPlanningService::new(pool.clone());
    let replanner_turn = service
        .start_replanning_for_signal(&task_id, "sig_replan")
        .await
        .expect("start replanner");
    assert_eq!(replanner_turn.profile_id, "replanner");

    let output = json!({
        "mode": "patch",
        "summary": "Add follow-up implementation",
        "patch": {
            "operations": [{
                "op": "add_work_item",
                "work_item": {
                    "temp_id": "followup",
                    "title": "Implement follow-up",
                    "description": "Address replanning signal",
                    "kind": "implementation",
                    "action": "agent_turn",
                    "execution_profile_id": "implementer",
                    "acceptance_criteria": ["Follow-up is complete"]
                }
            }]
        }
    });

    let outcome = service
        .submit_replanner_output(&task_id, &replanner_turn.session_id, output.to_string())
        .await
        .expect("submit replanner output");

    assert_eq!(outcome.proposal.mode, "patch");
    assert_eq!(outcome.proposal.state, "applied");
    assert_eq!(outcome.scheduler.dispatched_runs.len(), 1);

    let graph = SqliteDagGraphStore::new(pool.clone())
        .task_graph(&task_id)
        .await
        .expect("task graph");
    let followup_id = graph
        .work_items
        .iter()
        .find(|work_item| work_item.title == "Implement follow-up")
        .expect("followup work item")
        .work_item_id
        .clone();
    let followup_state: String = sqlx::query_scalar(
        "SELECT current_state FROM work_item_runtime_projection WHERE work_item_id = ?",
    )
    .bind(followup_id)
    .fetch_one(&pool)
    .await
    .expect("followup state");
    assert_eq!(followup_state, "running");

    let signal_states: Vec<(String, String)> = sqlx::query_as(
        "SELECT signal_id, state FROM dag_signals WHERE task_id = ? ORDER BY signal_id",
    )
    .bind(&task_id)
    .fetch_all(&pool)
    .await
    .expect("signal states");
    assert_eq!(
        signal_states,
        vec![
            ("sig_replan".to_string(), "acknowledged".to_string()),
            ("sig_unrelated".to_string(), "open".to_string())
        ]
    );
    let replanner_session_state: String =
        sqlx::query_scalar("SELECT state FROM sessions WHERE session_id = ?")
            .bind(&replanner_turn.session_id)
            .fetch_one(&pool)
            .await
            .expect("replanner session state");
    assert_eq!(replanner_session_state, "exited");

    cleanup_runtime_sessions(&pool).await;
}
