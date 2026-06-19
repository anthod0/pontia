use crate::agent_tools_support::*;
use axum::http::StatusCode;
use serde_json::json;

#[tokio::test]
async fn rejects_runtime_mismatch_and_non_dag_managed_turns() {
    let state = test_state().await;
    insert_task(&state.db(), "task_plan").await;
    insert_dag_session(
        &state.db(),
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
        &state.db(),
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
