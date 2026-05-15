#[path = "support/generic_client.rs"]
mod generic_client;
#[path = "support/http.rs"]
mod http;
#[path = "support/task_state.rs"]
mod task_state;

use axum::http::StatusCode;
use generic_client::GenericClientTestScope;
use http::{get_json, post_json};
use llmparty::{
    application::{DagSchedulerService, DagService, SubmitPlanPayload, WorkItemDraft},
    ids::new_task_id,
};
use serde_json::json;
use task_state::test_state;

async fn insert_running_task(state: &llmparty::application::AppState) -> String {
    let task_id = new_task_id().to_string();
    sqlx::query(
        "INSERT INTO tasks (task_id, state, input) VALUES (?, 'running', 'human control task')",
    )
    .bind(&task_id)
    .execute(&state.db)
    .await
    .expect("insert task");
    task_id
}

fn draft(temp_id: &str, priority: i64) -> WorkItemDraft {
    WorkItemDraft {
        temp_id: Some(temp_id.to_string()),
        title: format!("{temp_id} title"),
        description: format!("{temp_id} description"),
        kind: "implementation".to_string(),
        action: "agent_turn".to_string(),
        execution_profile_id: "implementer".to_string(),
        execution_profile_version: None,
        priority,
        optional: false,
        parallelizable: true,
        acceptance_criteria: vec!["done".to_string()],
        metadata: json!({}),
    }
}

fn initial_plan() -> SubmitPlanPayload {
    SubmitPlanPayload {
        mode: "initial_dag".to_string(),
        summary: "initial plan".to_string(),
        work_items: vec![draft("first", 10), draft("second", 5)],
        edges: vec![],
        assumptions: vec![],
        risks: vec![],
    }
}

#[tokio::test]
async fn human_signal_api_records_human_objection_visible_in_signals() {
    let _scope = GenericClientTestScope::new().await;
    let state = test_state().await;
    let task_id = insert_running_task(&state).await;

    let (status, body) = post_json(
        state.clone(),
        &format!("/external/v1/tasks/{task_id}/signals"),
        json!({
            "kind": "user_objection",
            "summary": "Plan is too broad",
            "detail": "Please reduce to one implementation step",
            "severity": "medium"
        }),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    let signal = &body["data"]["signal"];
    assert_eq!(signal["task_id"], task_id);
    assert_eq!(signal["source"], "human");
    assert_eq!(signal["kind"], "user_objection");
    assert_eq!(signal["summary"], "Plan is too broad");

    let (list_status, list_body) =
        get_json(state, &format!("/external/v1/tasks/{task_id}/signals")).await;
    assert_eq!(list_status, StatusCode::OK);
    assert_eq!(list_body["data"]["signals"][0]["source"], "human");
    assert_eq!(list_body["data"]["signals"][0]["state"], "open");
}

#[tokio::test]
async fn pause_prevents_scheduler_dispatch_until_resume() {
    let scope = GenericClientTestScope::new().await;
    let state = test_state().await;
    scope.enable_builtin_profiles(&state).await;
    let task_id = insert_running_task(&state).await;
    DagService::new(state.db.clone())
        .apply_initial_dag(&task_id, &initial_plan())
        .await
        .expect("apply dag");

    let (pause_status, pause_body) = post_json(
        state.clone(),
        &format!("/external/v1/tasks/{task_id}/pause"),
        json!({}),
    )
    .await;
    assert_eq!(pause_status, StatusCode::OK);
    assert_eq!(pause_body["data"]["task"]["state"], "paused");

    let paused_outcome = DagSchedulerService::new(state.db.clone())
        .schedule_task(&task_id)
        .await
        .expect("scheduler respects pause");
    assert_eq!(paused_outcome.dispatched_runs.len(), 0);

    let run_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM work_item_runs WHERE task_id = ?")
            .bind(&task_id)
            .fetch_one(&state.db)
            .await
            .expect("run count");
    assert_eq!(run_count, 0);

    let (resume_status, resume_body) = post_json(
        state.clone(),
        &format!("/external/v1/tasks/{task_id}/resume"),
        json!({}),
    )
    .await;
    assert_eq!(resume_status, StatusCode::OK);
    assert_eq!(resume_body["data"]["task"]["state"], "running");
    assert_eq!(
        resume_body["data"]["scheduler"]["dispatched_runs"]
            .as_array()
            .unwrap()
            .len(),
        2
    );

    for run in resume_body["data"]["scheduler"]["dispatched_runs"]
        .as_array()
        .unwrap()
    {
        assert!(run["session_id"].as_str().is_some());
    }
}
