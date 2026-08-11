use super::*;

#[tokio::test]
async fn otlp_tool_decision_authenticates_and_resolves_the_matching_approval_once() {
    let state = configured_otel_state("approval-otlp-once.db").await;

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
    let fixture = tool_decision_fixture();

    let missing_auth = post_otlp(state.clone(), fixture.clone(), None).await;
    assert_eq!(missing_auth.status(), StatusCode::UNAUTHORIZED);
    assert!(!hook_request.is_finished());
    let response = post_otlp(state.clone(), fixture.clone(), Some("Bearer test-token")).await;
    assert_eq!(response.status(), StatusCode::OK);

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
    assert_eq!(hook_body["data"]["result"], "resolved_elsewhere");

    let final_event: (String, String, String) = sqlx::query_as(
        r#"SELECT event_type, source, payload
           FROM events
           WHERE event_type IN ('approval.accepted', 'approval.rejected', 'approval.cancelled')"#,
    )
    .fetch_one(&state.db())
    .await
    .unwrap();
    assert_eq!(final_event.0, "approval.accepted");
    assert_eq!(final_event.1, "agent_client");
    let payload: Value = serde_json::from_str(&final_event.2).unwrap();
    assert_eq!(payload["request_event_id"], request_event_id);
    assert_eq!(payload["scope"], "once");
    assert_eq!(payload["client_session_id"], "claude_native");
    assert_eq!(payload["prompt_id"], "prompt_1");
    assert_eq!(payload["tool_name"], "Bash");
    assert_eq!(payload["tool_use_id"], "toolu_01PontiaApproval");
    assert!(payload.get("source").is_none());
    assert_eq!(
        payload.as_object().unwrap().len(),
        6,
        "only bounded correlation and decision metadata may be persisted"
    );

    let metadata: String =
        sqlx::query_scalar("SELECT metadata FROM sessions WHERE session_id = 'sess_approval'")
            .fetch_one(&state.db())
            .await
            .unwrap();
    assert!(
        serde_json::from_str::<Value>(&metadata).unwrap()["interaction"].is_null(),
        "final Approval must clear the projected interaction"
    );

    let duplicate = post_otlp(state.clone(), fixture, Some("Bearer test-token")).await;
    assert_eq!(duplicate.status(), StatusCode::OK);
    let final_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM events WHERE event_type IN ('approval.accepted', 'approval.rejected', 'approval.cancelled')",
    )
    .fetch_one(&state.db())
    .await
    .unwrap();
    assert_eq!(final_count, 1);
}

#[tokio::test]
async fn otlp_tool_decision_maps_all_final_decisions_without_persisting_the_otel_source() {
    for (index, decision, source, expected_event, expected_scope) in [
        (
            0,
            "accept",
            "user_permanent",
            "approval.accepted",
            Some("always"),
        ),
        (1, "accept", "config", "approval.accepted", Some("unknown")),
        (2, "accept", "hook", "approval.accepted", Some("unknown")),
        (3, "reject", "user_abort", "approval.cancelled", None),
        (4, "reject", "user_reject", "approval.rejected", None),
        (5, "reject", "config", "approval.rejected", None),
    ] {
        let state = configured_otel_state(&format!("approval-otlp-map-{index}.db")).await;
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
        let mut fixture = tool_decision_fixture();
        set_tool_decision_attribute(&mut fixture, "decision", decision);
        set_tool_decision_attribute(&mut fixture, "source", source);

        let response = post_otlp(state.clone(), fixture, Some("Bearer test-token")).await;
        assert_eq!(response.status(), StatusCode::OK);
        tokio::time::timeout(Duration::from_secs(2), hook_request)
            .await
            .expect("hook waiter release")
            .expect("hook task");

        let final_event: (String, String) = sqlx::query_as(
            r#"SELECT event_type, payload
               FROM events
               WHERE event_type IN (
                   'approval.accepted',
                   'approval.rejected',
                   'approval.cancelled'
               )"#,
        )
        .fetch_one(&state.db())
        .await
        .unwrap();
        assert_eq!(final_event.0, expected_event);
        let payload: Value = serde_json::from_str(&final_event.1).unwrap();
        assert_eq!(payload["request_event_id"], request_event_id);
        assert_eq!(payload.get("scope").and_then(Value::as_str), expected_scope);
        assert!(payload.get("source").is_none());
    }
}

#[tokio::test]
async fn hook_accept_uses_only_the_web_scope_delivered_by_the_current_process() {
    for (index, command, expected_scope) in [
        (0, json!({"decision": "accept_once"}), "once"),
        (
            1,
            json!({
                "decision": "always_allow",
                "permission_suggestion": {
                    "type": "addRules",
                    "rules": [{"toolName": "Bash", "ruleContent": "pnpm test"}],
                    "behavior": "allow",
                    "destination": "localSettings"
                }
            }),
            "always",
        ),
    ] {
        let state = configured_otel_state(&format!("approval-otlp-hook-{index}.db")).await;
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
        let command_uri =
            format!("/external/v1/sessions/sess_approval/approvals/{request_event_id}/decision");
        let command_response =
            post_external(state.clone(), &command_uri, command, "web-hook-scope").await;
        assert_eq!(command_response.status(), StatusCode::OK);
        tokio::time::timeout(Duration::from_secs(2), hook_request)
            .await
            .expect("web command releases hook")
            .expect("hook task");

        let mut fixture = tool_decision_fixture();
        set_tool_decision_attribute(&mut fixture, "source", "hook");
        let response = post_otlp(state.clone(), fixture, Some("Bearer test-token")).await;
        assert_eq!(response.status(), StatusCode::OK);

        let payload: String =
            sqlx::query_scalar("SELECT payload FROM events WHERE event_type = 'approval.accepted'")
                .fetch_one(&state.db())
                .await
                .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&payload).unwrap()["scope"],
            expected_scope
        );
    }
}

#[tokio::test]
async fn otlp_drops_unrelated_unbound_and_uncorrelated_records_as_unknown() {
    let state = configured_otel_state("approval-otlp-drop.db").await;
    let request_state = state.clone();
    let hook_request = tokio::spawn(async move {
        post(
            request_state,
            "/internal/v1/claude/permission-request",
            permission_request_body(),
        )
        .await
    });
    approval_event_id(&state).await;

    let mut contradictory_identity = tool_decision_fixture();
    contradictory_identity["resourceLogs"][0]["scopeLogs"][0]["logRecords"][0]["eventName"] =
        Value::String("claude_code.tool_result".to_string());
    assert_eq!(
        post_otlp(
            state.clone(),
            contradictory_identity,
            Some("Bearer test-token")
        )
        .await
        .status(),
        StatusCode::OK
    );

    for (key, value) in [
        ("session.id", "unbound_claude"),
        ("prompt.id", "other_prompt"),
        ("tool_name", "Write"),
    ] {
        let mut fixture = tool_decision_fixture();
        set_tool_decision_attribute(&mut fixture, key, value);
        assert_eq!(
            post_otlp(state.clone(), fixture, Some("Bearer test-token"))
                .await
                .status(),
            StatusCode::OK
        );
    }
    assert!(!hook_request.is_finished());
    let final_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM events WHERE event_type IN ('approval.accepted', 'approval.rejected', 'approval.cancelled')",
    )
    .fetch_one(&state.db())
    .await
    .unwrap();
    assert_eq!(final_count, 0);

    let response = post_otlp(
        state.clone(),
        tool_decision_fixture(),
        Some("Bearer test-token"),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    tokio::time::timeout(Duration::from_secs(2), hook_request)
        .await
        .expect("valid observation releases hook")
        .expect("hook task");
}

#[tokio::test]
async fn sequential_approvals_wait_for_the_previous_otlp_final_before_registering() {
    let state = configured_otel_state("approval-otlp-sequential.db").await;
    let first_state = state.clone();
    let first_hook = tokio::spawn(async move {
        post(
            first_state,
            "/internal/v1/claude/permission-request",
            permission_request_body(),
        )
        .await
    });
    let first_request_event_id = approval_event_id(&state).await;
    let command_uri =
        format!("/external/v1/sessions/sess_approval/approvals/{first_request_event_id}/decision");
    let command = post_external(
        state.clone(),
        &command_uri,
        json!({"decision": "accept_once"}),
        "first-web-decision",
    )
    .await;
    assert_eq!(command.status(), StatusCode::OK);
    tokio::time::timeout(Duration::from_secs(2), first_hook)
        .await
        .expect("first web command releases hook")
        .expect("first hook task");

    let second_state = state.clone();
    let second_hook = tokio::spawn(async move {
        post(
            second_state,
            "/internal/v1/claude/permission-request",
            permission_request_body(),
        )
        .await
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    let requested_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE event_type = 'approval.requested'")
            .fetch_one(&state.db())
            .await
            .unwrap();
    assert_eq!(
        requested_count, 1,
        "a second durable request would make OTLP correlation ambiguous"
    );
    assert!(!second_hook.is_finished());

    let first_final = post_otlp(
        state.clone(),
        tool_decision_fixture(),
        Some("Bearer test-token"),
    )
    .await;
    assert_eq!(first_final.status(), StatusCode::OK);
    let second_request_event_id = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if let Some(event_id) = sqlx::query_scalar::<_, String>(
                r#"SELECT event_id
                   FROM events
                   WHERE event_type = 'approval.requested' AND event_id != ?"#,
            )
            .bind(&first_request_event_id)
            .fetch_optional(&state.db())
            .await
            .unwrap()
            {
                break event_id;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("second Approval registration");
    assert_ne!(second_request_event_id, first_request_event_id);

    let second_final = post_otlp(
        state.clone(),
        tool_decision_fixture(),
        Some("Bearer test-token"),
    )
    .await;
    assert_eq!(second_final.status(), StatusCode::OK);
    tokio::time::timeout(Duration::from_secs(2), second_hook)
        .await
        .expect("second OTLP final releases hook")
        .expect("second hook task");
    let final_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE event_type = 'approval.accepted'")
            .fetch_one(&state.db())
            .await
            .unwrap();
    assert_eq!(final_count, 2);
}

#[tokio::test]
async fn otlp_requires_exactly_one_unresolved_approval_before_correlating() {
    let state = configured_otel_state("approval-otlp-ambiguous.db").await;
    let request_state = state.clone();
    let hook_request = tokio::spawn(async move {
        post(
            request_state,
            "/internal/v1/claude/permission-request",
            permission_request_body(),
        )
        .await
    });
    approval_event_id(&state).await;
    sqlx::query(
        r#"INSERT INTO events (
               event_id,
               session_id,
               turn_id,
               source,
               client_type,
               event_type,
               occurred_at,
               payload
           )
           VALUES (
               'evt_ambiguous_approval',
               'sess_approval',
               'turn_approval',
               'agent_client',
               'claude',
               'approval.requested',
               '2026-07-30T00:00:00Z',
               '{"client_session_id":"claude_native","prompt_id":"prompt_1","tool_name":"Bash","permission_suggestions":[]}'
           )"#,
    )
    .execute(&state.db())
    .await
    .unwrap();

    let ambiguous = post_otlp(
        state.clone(),
        tool_decision_fixture(),
        Some("Bearer test-token"),
    )
    .await;
    assert_eq!(ambiguous.status(), StatusCode::OK);
    assert!(!hook_request.is_finished());
    let final_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM events WHERE event_type IN ('approval.accepted', 'approval.rejected', 'approval.cancelled')",
    )
    .fetch_one(&state.db())
    .await
    .unwrap();
    assert_eq!(final_count, 0);

    sqlx::query("DELETE FROM events WHERE event_id = 'evt_ambiguous_approval'")
        .execute(&state.db())
        .await
        .unwrap();
    let mut without_tool_use_id = tool_decision_fixture();
    without_tool_use_id["resourceLogs"][0]["scopeLogs"][0]["logRecords"][0]["attributes"]
        .as_array_mut()
        .unwrap()
        .retain(|attribute| attribute["key"] != "tool_use_id");
    let correlated = post_otlp(
        state.clone(),
        without_tool_use_id,
        Some("Bearer test-token"),
    )
    .await;
    assert_eq!(correlated.status(), StatusCode::OK);
    tokio::time::timeout(Duration::from_secs(2), hook_request)
        .await
        .expect("unambiguous observation releases hook")
        .expect("hook task");
    let payload: String =
        sqlx::query_scalar("SELECT payload FROM events WHERE event_type = 'approval.accepted'")
            .fetch_one(&state.db())
            .await
            .unwrap();
    assert!(
        serde_json::from_str::<Value>(&payload)
            .unwrap()
            .get("tool_use_id")
            .is_none(),
        "tool_use_id is optional audit metadata, not a correlation key"
    );
}
