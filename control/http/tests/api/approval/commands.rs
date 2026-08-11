use super::{
    BodyExt, Duration, StatusCode, Value, approval_event_id, configured_otel_state, json,
    permission_request_body, post, post_external,
};

#[tokio::test]
async fn approval_commands_deliver_each_decision_without_projecting_a_final_fact() {
    for (database, decision, expected) in [
        (
            "approval-accept-once.db",
            json!({"decision": "accept_once"}),
            json!({"decision": "accept_once"}),
        ),
        (
            "approval-reject.db",
            json!({"decision": "reject"}),
            json!({"decision": "reject"}),
        ),
        (
            "approval-always-allow.db",
            json!({
                "decision": "always_allow",
                "permission_suggestion": {
                    "type": "addRules",
                    "rules": [{"toolName": "Bash", "ruleContent": "pnpm test"}],
                    "behavior": "allow",
                    "destination": "localSettings"
                }
            }),
            json!({
                "decision": "always_allow",
                "permission_suggestion": {
                    "type": "addRules",
                    "rules": [{"toolName": "Bash", "ruleContent": "pnpm test"}],
                    "behavior": "allow",
                    "destination": "localSettings"
                }
            }),
        ),
    ] {
        let state = configured_otel_state(database).await;

        let request_state = state.clone();
        let hook_request = tokio::spawn(async move {
            post(
                request_state,
                "/internal/v1/claude/permission-request",
                permission_request_body(),
            )
            .await
        });
        let request_event_id = approval_event_id(&state).await;
        let uri =
            format!("/external/v1/sessions/sess_approval/approvals/{request_event_id}/decision");

        let response =
            post_external(state.clone(), &uri, decision.clone(), "approval-command").await;
        assert_eq!(response.status(), StatusCode::OK);
        let response_body: Value =
            serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        assert_eq!(response_body["data"]["request_event_id"], request_event_id);
        assert_eq!(response_body["data"]["delivered"], true);

        let hook_response = tokio::time::timeout(Duration::from_secs(2), hook_request)
            .await
            .expect("hook waiter release")
            .expect("hook task");
        let hook_body: Value = serde_json::from_slice(
            &hook_response
                .into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes(),
        )
        .unwrap();
        assert_eq!(hook_body["data"]["result"], expected);

        let metadata: String =
            sqlx::query_scalar("SELECT metadata FROM sessions WHERE session_id = 'sess_approval'")
                .fetch_one(&state.db())
                .await
                .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&metadata).unwrap()["interaction"]["request_event_id"],
            request_event_id
        );
        let final_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM events WHERE event_type IN ('approval.accepted', 'approval.rejected', 'approval.cancelled')",
        )
        .fetch_one(&state.db())
        .await
        .unwrap();
        assert_eq!(final_count, 0);

        let duplicate = post_external(state.clone(), &uri, decision, "approval-command").await;
        assert_eq!(duplicate.status(), StatusCode::OK);
        let stale = post_external(
            state.clone(),
            &uri,
            json!({"decision": "reject"}),
            "different-command",
        )
        .await;
        assert_eq!(stale.status(), StatusCode::CONFLICT);
    }
}

#[tokio::test]
async fn approval_command_rejects_a_mutated_suggestion_or_wrong_session_without_waking_the_waiter()
{
    let state = configured_otel_state("approval-exact-suggestion.db").await;

    let request_state = state.clone();
    let hook_request = tokio::spawn(async move {
        post(
            request_state,
            "/internal/v1/claude/permission-request",
            permission_request_body(),
        )
        .await
    });
    let request_event_id = approval_event_id(&state).await;
    let uri = format!("/external/v1/sessions/sess_approval/approvals/{request_event_id}/decision");
    let mutated = post_external(
        state.clone(),
        &uri,
        json!({
            "decision": "always_allow",
            "permission_suggestion": {
                "type": "addRules",
                "rules": [{"toolName": "Bash", "ruleContent": "pnpm *"}],
                "behavior": "allow",
                "destination": "localSettings"
            }
        }),
        "mutated",
    )
    .await;
    assert_eq!(mutated.status(), StatusCode::CONFLICT);
    assert!(!hook_request.is_finished());

    let wrong_session = post_external(
        state.clone(),
        &format!("/external/v1/sessions/other/approvals/{request_event_id}/decision"),
        json!({"decision": "accept_once"}),
        "wrong-session",
    )
    .await;
    assert_eq!(wrong_session.status(), StatusCode::CONFLICT);
    assert!(!hook_request.is_finished());

    let accepted = post_external(
        state,
        &uri,
        json!({"decision": "reject"}),
        "reject-after-conflicts",
    )
    .await;
    assert_eq!(accepted.status(), StatusCode::OK);
    hook_request.await.unwrap();
}
