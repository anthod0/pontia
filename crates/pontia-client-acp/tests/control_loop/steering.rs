use super::*;

async fn submit(h: &Harness, session: &str, policy: &str) -> Value {
    h.app
        .inbox_commands()
        .submit_message(
            session,
            SubmitInboxMessageRequest {
                input: "end_turn".into(),
                delivery_policy: policy.into(),
                branch_target_turn_id: None,
                metadata: json!({}),
            },
        )
        .await
        .unwrap()
        .data["inbox_message"]
        .clone()
}

async fn start(h: &Harness, session: &str) -> String {
    let message = submit(h, session, "steer").await;
    let turn = message["turn_id"].as_str().unwrap().to_owned();
    h.release("update-1");
    h.wait_turn(session, &turn, "running").await;
    turn
}

async fn finish_queued(h: &Harness, session: &str, turn: &str, message: &Value) {
    h.release("release-1");
    h.wait_turn(session, turn, "completed").await;
    tokio::time::timeout(Duration::from_secs(5), async {
        while h.requests("session/prompt") != 2 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    h.release("update-2");
    h.release("release-2");
    tokio::time::timeout(Duration::from_secs(5), async {
        while h.events(session, "turn.completed").await != 2 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    let delivered = h
        .app
        .inbox_commands()
        .get_message(session, message["message_id"].as_str().unwrap())
        .await
        .unwrap()
        .unwrap();
    let delivered = serde_json::to_value(delivered).unwrap();
    assert_eq!(delivered["state"], "dispatched");
    assert_ne!(delivered["turn_id"], turn);
    assert_eq!(h.requests("session/cancel"), 0);
    assert_eq!(h.events(session, "inbox.message_failed").await, 0);
    assert!(!h.wire().iter().any(|item| item["overlap"] == true));
}

#[tokio::test]
async fn unsupported_steer_silently_waits_in_inbox_until_idle() {
    let h = Harness::new(&[]).await;
    let session = h.create("first").await.unwrap();
    assert!(
        !h.app
            .queries()
            .get_session(&session)
            .await
            .unwrap()
            .unwrap()
            .capabilities
            .steer
    );
    let turn = start(&h, &session).await;
    let message = submit(&h, &session, "steer").await;
    assert_eq!(message["state"], "pending");
    assert_eq!(message["delivery_policy"], "after_idle");
    assert!(message["failure_message"].is_null());
    assert!(message["turn_id"].is_null());
    assert_eq!(h.requests("_session/steering"), 0);
    assert_eq!(h.requests("session/prompt"), 1);
    finish_queued(&h, &session, &turn, &message).await;
    h.stop().await;
}

#[tokio::test]
async fn advertised_steering_joins_current_turn_and_after_idle_waits() {
    let mut h = Harness::new(&[("ACP_STEERING", "1")]).await;
    let session = h.create("first").await.unwrap();
    assert!(
        h.app
            .queries()
            .get_session(&session)
            .await
            .unwrap()
            .unwrap()
            .capabilities
            .steer
    );
    let turn = start(&h, &session).await;
    let queued = submit(&h, &session, "after_idle").await;
    for _ in 0..2 {
        let message = submit(&h, &session, "steer").await;
        assert_eq!(message["state"], "dispatched");
        assert_eq!(message["turn_id"], turn);
    }
    assert_eq!(h.requests("_session/steering"), 2);
    assert_eq!(h.requests("session/prompt"), 1);
    let items =
        serde_json::to_value(h.app.live_output().snapshot(&session, &turn).unwrap().items).unwrap();
    let inputs: Vec<_> = items
        .as_array()
        .unwrap()
        .iter()
        .filter(|item| item["kind"] == "user_input")
        .map(|item| item["text"].as_str().unwrap())
        .collect();
    assert_eq!(inputs, ["end_turn", "end_turn"]);
    assert_eq!(h.events(&session, "turn.started").await, 1);
    assert_eq!(h.events(&session, "turn.completed").await, 0);
    for request in h
        .wire()
        .iter()
        .filter(|item| item["method"] == "_session/steering")
    {
        assert_eq!(request["params"]["sessionId"], "native-id");
        assert_eq!(
            request["params"]["prompt"],
            json!([{"type":"text","text":"end_turn"}])
        );
        assert_eq!(
            request["params"]["_meta"]["steering"]["idleBehavior"],
            "promptRequired"
        );
    }
    finish_queued(&h, &session, &turn, &queued).await;
    for restart in [false, true] {
        if restart {
            h.restart().await;
            h.wait_ready(&session, 2).await;
        }
        let history = h.history(&session).await.unwrap();
        let first_turn_text: String = history
            .items
            .iter()
            .filter(|item| item.turn_id.as_deref() == Some(turn.as_str()))
            .map(|item| item.item.content_preview.as_str())
            .collect();
        assert_eq!(first_turn_text.matches("end_turn").count(), 3);
        assert_eq!(first_turn_text.matches("streamed answer").count(), 1);
        assert!(
            h.app
                .queries()
                .get_session(&session)
                .await
                .unwrap()
                .unwrap()
                .capabilities
                .steer
        );
    }
    h.stop().await;
}

#[tokio::test]
async fn prompt_required_keeps_unconsumed_steer_queued() {
    let h = Harness::new(&[
        ("ACP_STEERING", "1"),
        ("ACP_STEERING_OUTCOME", "promptRequired"),
    ])
    .await;
    let session = h.create("first").await.unwrap();
    let turn = start(&h, &session).await;
    let message = submit(&h, &session, "steer").await;
    assert_eq!(message["state"], "pending");
    assert_eq!(message["delivery_policy"], "after_idle");
    assert!(message["failure_message"].is_null());
    assert_eq!(h.requests("_session/steering"), 1);
    assert_eq!(h.requests("session/prompt"), 1);
    finish_queued(&h, &session, &turn, &message).await;
    assert_eq!(h.requests("_session/steering"), 1);
    h.stop().await;
}

#[tokio::test]
async fn lost_steering_receipt_is_unknown_and_not_replayed() {
    let h = Harness::new(&[
        ("ACP_STEERING", "1"),
        ("ACP_STEERING_OUTCOME", "disconnect"),
    ])
    .await;
    let session = h.create("first").await.unwrap();
    let turn = start(&h, &session).await;
    let message = submit(&h, &session, "steer").await;
    assert_eq!(message["state"], "unknown");
    assert_eq!(h.requests("_session/steering"), 1);
    h.app.inbox_commands().drain_inbox(&session).await.unwrap();
    assert_eq!(h.requests("session/prompt"), 1);
    assert_eq!(h.requests("_session/steering"), 1);
    assert_eq!(
        h.app
            .queries()
            .get_turn(&session, &turn)
            .await
            .unwrap()
            .unwrap()
            .state,
        "running"
    );
    h.stop().await;
}

#[tokio::test]
async fn live_input_queues_only_when_the_busy_peer_lacks_steering() {
    use pontia_agent_control::LiveInputDelivery;
    for supported in [false, true] {
        let h = Harness::new(&[("ACP_STEERING", if supported { "1" } else { "0" })]).await;
        let session = h.create("first").await.unwrap();
        let inbox = h.app.inbox_commands();
        assert_eq!(
            inbox
                .submit_live_input(&session, "end_turn".into())
                .await
                .unwrap(),
            LiveInputDelivery::Accepted
        );
        h.release("update-1");
        h.wait_requests("session/prompt", 1).await;
        let turn = sqlx::query_scalar::<_, String>("SELECT turn_id FROM turns WHERE session_id=?")
            .bind(&session)
            .fetch_one(&h.app.db())
            .await
            .unwrap();
        h.wait_turn(&session, &turn, "running").await;
        let receipt = inbox
            .submit_live_input(&session, "end_turn".into())
            .await
            .unwrap();
        assert_eq!(h.requests("session/prompt"), 1);
        let messages = inbox.list_messages(&session).await.unwrap();
        if supported {
            assert_eq!(receipt, LiveInputDelivery::Accepted);
            assert_eq!(h.requests("_session/steering"), 1);
            assert!(messages.is_empty());
            h.release("release-1");
            h.wait_turn(&session, &turn, "completed").await;
        } else {
            assert_eq!(receipt, LiveInputDelivery::Queued);
            assert_eq!(messages.len(), 1);
            assert_eq!(messages[0].state, "pending");
            finish_queued(
                &h,
                &session,
                &turn,
                &serde_json::to_value(&messages[0]).unwrap(),
            )
            .await;
        }
        h.stop().await;
    }
}
