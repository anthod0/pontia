#![cfg(any())]

#[path = "support/generic_client.rs"]
mod generic_client;
#[path = "support/http.rs"]
mod http;
#[path = "support/task_state.rs"]
mod task_state;

use axum::http::StatusCode;
use generic_client::GenericClientTestScope;
use http::{get_json, post_json};
use pilotfy::{
    application::{DagService, SubmitPlanPayload, WorkItemDraft, WorkItemEdgeDraft},
    ids::new_task_id,
};
use serde_json::json;
use task_state::test_state;

async fn insert_running_task(state: &pilotfy::application::AppState) -> String {
    let task_id = new_task_id().to_string();
    sqlx::query("INSERT INTO tasks (task_id, state, input) VALUES (?, 'running', 'dag api task')")
        .bind(&task_id)
        .execute(&state.db)
        .await
        .expect("insert task");
    task_id
}

fn draft(temp_id: &str, profile: &str, priority: i64) -> WorkItemDraft {
    WorkItemDraft {
        temp_id: Some(temp_id.to_string()),
        title: format!("{temp_id} title"),
        description: format!("{temp_id} description"),
        kind: "implementation".to_string(),
        action: "agent_turn".to_string(),
        execution_profile_id: profile.to_string(),
        execution_profile_version: None,
        priority,
        optional: false,
        parallelizable: true,
        acceptance_criteria: vec!["done".to_string()],
        metadata: json!({}),
    }
}

fn edge(from: &str, to: &str) -> WorkItemEdgeDraft {
    WorkItemEdgeDraft {
        from_work_item_id: from.to_string(),
        to_work_item_id: to.to_string(),
        edge_type: "depends_on".to_string(),
    }
}

fn initial_plan() -> SubmitPlanPayload {
    SubmitPlanPayload {
        mode: "initial_dag".to_string(),
        summary: "initial plan".to_string(),
        work_items: vec![
            draft("design", "planner", 5),
            draft("impl", "implementer", 1),
        ],
        edges: vec![edge("design", "impl")],
        assumptions: vec![],
        risks: vec![],
    }
}

#[tokio::test]
async fn dag_external_api_exposes_summary_work_items_runs_and_signals_from_projections() {
    let scope = GenericClientTestScope::new().await;
    let state = test_state().await;
    scope.enable_builtin_profiles(&state).await;
    let task_id = insert_running_task(&state).await;
    DagService::new(state.db.clone())
        .apply_initial_dag(&task_id, &initial_plan())
        .await
        .expect("apply dag");

    let (tick_status, tick_body) = post_json(
        state.clone(),
        &format!("/external/v1/tasks/{task_id}/scheduler/tick"),
        json!({}),
    )
    .await;
    assert_eq!(tick_status, StatusCode::OK);
    let dispatched = tick_body["data"]["scheduler"]["dispatched_runs"]
        .as_array()
        .expect("dispatched runs");
    assert_eq!(dispatched.len(), 1);
    let run_id = dispatched[0]["run_id"].as_str().expect("run id");
    let work_item_id = dispatched[0]["work_item_id"]
        .as_str()
        .expect("work item id");
    let session_id = dispatched[0]["session_id"].as_str().expect("session id");
    let turn_id = dispatched[0]["turn_id"].as_str().expect("turn id");

    sqlx::query(
        r#"INSERT INTO dag_signals (
                signal_id, task_id, work_item_id, run_id, source_session_id,
                kind, summary, detail, severity, related_refs
           ) VALUES ('dagsig_api_1', ?, ?, ?, ?, 'risk', 'API risk', 'watch projection drift', 'medium', '[]')"#,
    )
    .bind(&task_id)
    .bind(work_item_id)
    .bind(run_id)
    .bind(session_id)
    .execute(&state.db)
    .await
    .expect("insert signal");

    let (dag_status, dag_body) =
        get_json(state.clone(), &format!("/external/v1/tasks/{task_id}/dag")).await;
    assert_eq!(dag_status, StatusCode::OK);
    let dag = &dag_body["data"]["dag"];
    assert_eq!(dag["task_id"], task_id);
    assert_eq!(dag["summary"]["total_work_items"], 2);
    assert_eq!(dag["summary"]["running_work_items"], 1);
    assert_eq!(dag["work_items"].as_array().unwrap().len(), 2);
    assert_eq!(dag["edges"].as_array().unwrap().len(), 1);
    assert_eq!(dag["runs"].as_array().unwrap().len(), 1);
    assert_eq!(dag["signals"].as_array().unwrap().len(), 1);
    assert_eq!(dag["runs"][0]["turn_id"], turn_id);

    let (items_status, items_body) = get_json(
        state.clone(),
        &format!("/external/v1/tasks/{task_id}/work-items"),
    )
    .await;
    assert_eq!(items_status, StatusCode::OK);
    assert_eq!(
        items_body["data"]["work_items"].as_array().unwrap().len(),
        2
    );
    assert!(
        items_body["data"]["work_items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item.get("runtime").is_some())
    );

    let (runs_status, runs_body) = get_json(
        state.clone(),
        &format!("/external/v1/tasks/{task_id}/work-item-runs"),
    )
    .await;
    assert_eq!(runs_status, StatusCode::OK);
    assert_eq!(runs_body["data"]["runs"][0]["session_id"], session_id);
    assert_eq!(runs_body["data"]["runs"][0]["turn_id"], turn_id);

    let (signals_status, signals_body) =
        get_json(state, &format!("/external/v1/tasks/{task_id}/signals")).await;
    assert_eq!(signals_status, StatusCode::OK);
    assert_eq!(signals_body["data"]["signals"][0]["summary"], "API risk");
}

#[tokio::test]
async fn dag_external_api_returns_not_found_for_missing_task() {
    let _scope = GenericClientTestScope::new().await;
    let state = test_state().await;

    let (status, body) = get_json(state, "/external/v1/tasks/task_missing/dag").await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "not_found");
}
