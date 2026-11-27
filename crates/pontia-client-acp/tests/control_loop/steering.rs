use super::*;

async fn submit(h: &Harness, session: &str, policy: &str) -> Value {
    h.app
        .inbox_commands()
        .submit_message(
            session,
            SubmitInboxMessageRequest {
                input: "end_turn".into(),
                delivery_policy: policy.into(),
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

async fn expect_pending_history(h: &Harness, session: &str) {
    assert!(
        h.app
            .queries()
            .get_session(session)
            .await
            .unwrap()
            .unwrap()
            .capabilities
            .timeline
    );
    assert!(matches!(
        h.history(session).await,
        Err(pontia_agent_control::TurnTimelineServiceError::Pending)
    ));
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
async fn antigravity_finishes_the_running_turn_with_the_latest_prompt() {
    let mut h = Harness::new(&[("ACP_ANTIGRAVITY", "1")]).await;
    let session = h.create("antigravity").await.unwrap();
    // A superseded response must not fail the Turn, even after reattachment.
    let turn = h
        .app
        .turn_commands()
        .start_turn(&session, "max_tokens".into(), json!({}))
        .await
        .unwrap()
        .unwrap()
        .turn_id;
    h.release("update-1");
    h.wait_turn(&session, &turn, "running").await;
    let message = submit(&h, &session, "steer").await;
    assert_eq!(message["turn_id"], turn);
    assert_eq!(message["state"], "dispatched");
    expect_pending_history(&h, &session).await;
    h.release("release-1");
    h.release("update-2");
    h.restart().await;
    h.release("release-2");
    h.wait_turn(&session, &turn, "completed").await;
    let timeline = h.history(&session).await.unwrap();
    let messages: Vec<_> = timeline
        .items
        .iter()
        .filter(|item| item.item.role == "user")
        .collect();
    assert_eq!(messages.len(), 2);
    assert!(
        messages
            .iter()
            .all(|item| item.turn_id.as_deref() == Some(turn.as_str()))
    );
    assert_eq!(h.events(&session, "turn.completed").await, 1);
    assert_eq!(h.requests("session/prompt"), 2);
    h.stop().await;
}

#[tokio::test]
async fn antigravity_replays_an_interrupted_prompt_before_the_next_input() {
    let h = Harness::new(&[("ACP_ANTIGRAVITY", "1")]).await;
    let session = h.create("antigravity").await.unwrap();
    let turn = start(&h, &session).await;
    assert_eq!(submit(&h, &session, "steer").await["turn_id"], turn);
    h.release("release-1");
    h.release("update-2");
    h.release("release-2");
    h.wait_turn(&session, &turn, "completed").await;

    // The native replay represents the interrupted output as a completed tool.
    let path = h.root.path().join("history.jsonl");
    let transcript = std::fs::read_to_string(&path).unwrap();
    let mut updates: Vec<Value> = transcript
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let assistant = updates
        .iter_mut()
        .find(|u| u["sessionUpdate"] == "agent_message_chunk")
        .unwrap();
    *assistant = json!({"sessionUpdate":"tool_call","toolCallId":"lookup","title":"Lookup","status":"completed"});
    let replay = updates
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    let steer = updates
        .iter_mut()
        .filter(|u| u["sessionUpdate"] == "user_message_chunk")
        .nth(1)
        .unwrap();
    steer["content"]["text"] = json!("wrong input");
    std::fs::write(
        &path,
        updates
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n"),
    )
    .unwrap();
    let turns = h.app.turn_commands();
    let dispatch = || turns.start_turn(&session, "end_turn".into(), json!({}));
    assert!(matches!(
        dispatch().await,
        Err(Error::CapabilityUnavailable(_))
    ));
    assert_eq!(h.requests("session/prompt"), 2);

    std::fs::write(path, replay).unwrap();
    let next = dispatch().await.unwrap().unwrap();
    h.release("update-3");
    h.release("release-3");
    h.wait_turn(&session, &next.turn_id, "completed").await;
    h.stop().await;
}

/// Runs one steered Antigravity Turn whose prompts are numbered from `first`.
async fn steered_turn(h: &Harness, session: &str, first: usize) {
    let turn = submit(h, session, "steer").await["turn_id"]
        .as_str()
        .unwrap()
        .to_owned();
    h.release(&format!("update-{first}"));
    h.wait_turn(session, &turn, "running").await;
    assert_eq!(submit(h, session, "steer").await["turn_id"], turn);
    h.release(&format!("release-{first}"));
    h.release(&format!("update-{}", first + 1));
    h.release(&format!("release-{}", first + 1));
    h.wait_turn(session, &turn, "completed").await;
}

#[tokio::test]
async fn antigravity_accepts_a_running_step_replayed_before_its_history() {
    let h = Harness::new(&[("ACP_ANTIGRAVITY", "1")]).await;
    let session = h.create("antigravity").await.unwrap();
    let path = h.root.path().join("history.jsonl");
    let server = |status: &str| {
        json!({"sessionUpdate":"tool_call","toolCallId":"server","title":"Serve","status":status})
            .to_string()
    };
    steered_turn(&h, &session, 1).await;

    // While a background command runs, agy replays its step before all history.
    let transcript = std::fs::read_to_string(&path).unwrap();
    std::fs::write(&path, format!("{}\n{transcript}", server("in_progress"))).unwrap();
    steered_turn(&h, &session, 3).await;

    // Once the command stops, its step returns to its native position.
    let transcript = std::fs::read_to_string(&path).unwrap();
    let mut lines: Vec<_> = transcript.lines().skip(1).collect();
    let stopped = server("failed");
    lines.insert(1, &stopped);
    std::fs::write(&path, lines.join("\n")).unwrap();
    let next = submit(&h, &session, "steer").await;
    h.release("update-5");
    h.release("release-5");
    h.wait_turn(&session, next["turn_id"].as_str().unwrap(), "completed")
        .await;
    assert_eq!(h.events(&session, "turn.dispatch_failed").await, 0);
    assert_eq!(h.requests("session/prompt"), 5);
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

#[tokio::test]
async fn missing_interject_queues_live_input_and_disables_only_this_peer() {
    let h = Harness::new(&[("ACP_INTERJECT", "unsupported")]).await;
    let session = h.create("first").await.unwrap();
    let turn = start(&h, &session).await;
    assert_eq!(h.requests("_x.ai/interject"), 0);
    let inbox = h.app.inbox_commands();
    assert_eq!(
        inbox
            .submit_live_input(&session, "end_turn".into())
            .await
            .unwrap(),
        pontia_agent_control::LiveInputDelivery::Queued
    );
    let messages = inbox.list_messages(&session).await.unwrap();
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
    finish_queued(
        &h,
        &session,
        &turn,
        &serde_json::to_value(messages.iter().find(|m| m.state == "pending").unwrap()).unwrap(),
    )
    .await;
    assert_eq!(h.requests("_x.ai/interject"), 1);
    h.stop().await;
}

#[tokio::test]
async fn native_interjections_keep_distinct_inputs_in_wire_and_replay_order() {
    for mode in ["echo-first", "receipt-first", "promote"] {
        let mut h = Harness::new(&[("ACP_INTERJECT", mode)]).await;
        let session = h.create("first").await.unwrap();
        let turn = start(&h, &session).await;
        let mut rows = vec![];
        for _ in 0..2 {
            let msg = h
                .app
                .inbox_commands()
                .submit_message(
                    &session,
                    SubmitInboxMessageRequest {
                        input: "再看一下🦓".into(),
                        metadata: json!({}),
                        delivery_policy: "steer".into(),
                    },
                )
                .await
                .unwrap()
                .data["inbox_message"]
                .clone();
            assert_eq!(msg["state"], "dispatched", "{mode} {msg}");
            assert!(msg["turn_id"].is_null());
            rows.push(msg["message_id"].as_str().unwrap().to_owned());
        }
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let snapshot = h.app.live_output().snapshot(&session, &turn).unwrap();
                if serde_json::to_value(snapshot.items)
                    .unwrap()
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|i| i["kind"] == "user_input")
                    .count()
                    == 2
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        expect_pending_history(&h, &session).await;
        if mode == "receipt-first" {
            h.restart().await;
        }
        h.release("release-1");
        let expected = if mode == "promote" { 3 } else { 1 };
        tokio::time::timeout(Duration::from_secs(5), async {
            while h.events(&session, "turn.completed").await != expected {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        let history = h.history(&session).await.unwrap();
        let extra: Vec<_> = history
            .items
            .iter()
            .filter(|i| i.item.content_preview == "再看一下🦓")
            .collect();
        assert_eq!(extra.len(), 2);
        if mode == "promote" {
            assert_ne!(extra[0].turn_id, extra[1].turn_id);
            assert!(extra.iter().all(|i| i.turn_id.as_deref() != Some(&turn)));
        } else {
            assert!(extra.iter().all(|i| i.turn_id.as_deref() == Some(&turn)));
            for row in rows {
                assert_eq!(
                    h.app
                        .inbox_commands()
                        .get_message(&session, &row)
                        .await
                        .unwrap()
                        .unwrap()
                        .turn_id
                        .as_deref(),
                    Some(turn.as_str())
                );
            }
        }
        assert_eq!(h.requests("session/prompt"), 1);
        assert_eq!(h.requests("session/cancel"), 0);
        h.stop().await;
    }
}

#[tokio::test]
async fn unrecognized_interjection_receipt_keeps_the_prompt_running_without_resubmission() {
    let h = Harness::new(&[("ACP_INTERJECT", "unknown")]).await;
    let session = h.create("first").await.unwrap();
    let turn = start(&h, &session).await;
    assert_eq!(submit(&h, &session, "steer").await["state"], "unknown");
    h.release("release-1");
    h.wait_turn(&session, &turn, "completed").await;
    h.app.inbox_commands().drain_inbox(&session).await.unwrap();
    assert_eq!(h.requests("session/prompt"), 1);
    assert_eq!(h.requests("_x.ai/interject"), 1);
    assert_eq!(h.requests("session/cancel"), 0);
    h.stop().await;
}

#[tokio::test]
async fn unavailable_interjection_method_returns_dashboard_message_to_inbox() {
    let h = Harness::new(&[("ACP_INTERJECT", "unsupported")]).await;
    let session = h.create("first").await.unwrap();
    let turn = start(&h, &session).await;
    let message = submit(&h, &session, "steer").await;
    assert_eq!(message["delivery_policy"], "after_idle");
    assert_eq!(message["state"], "pending");
    finish_queued(&h, &session, &turn, &message).await;
    assert_eq!(h.requests("_x.ai/interject"), 1);
    h.stop().await;
}

#[tokio::test]
async fn disconnected_interjection_is_unknown_and_never_resubmitted() {
    let h = Harness::new(&[("ACP_INTERJECT", "disconnect")]).await;
    let session = h.create("first").await.unwrap();
    start(&h, &session).await;
    assert_eq!(submit(&h, &session, "steer").await["state"], "unknown");
    h.app.inbox_commands().drain_inbox(&session).await.unwrap();
    assert_eq!(h.requests("session/prompt"), 1);
    assert_eq!(h.requests("_x.ai/interject"), 1);
    assert_eq!(h.requests("session/cancel"), 0);
    h.stop().await;
}

async fn wait_fixture_response(h: &Harness, id: &str) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while !h
            .wire()
            .iter()
            .any(|frame| frame["id"] == id && frame.get("error").is_some())
        {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn kiro_injects_into_the_running_turn_and_filters_live_and_replayed_notes() {
    let h = Harness::new(&[("ACP_KIRO", "1"), ("ACP_KIRO_STEER", "inject")]).await;
    let session = h.create("kiro").await.unwrap();
    let turn = start(&h, &session).await;
    let mut delivery = Box::pin(submit(&h, &session, "steer"));
    tokio::select! {
        _ = &mut delivery => panic!("steering completed before injection"),
        _ = wait_fixture_response(&h, "steer-queued-barrier") => {}
    }
    h.release("inject");
    let message = delivery.await;
    assert_eq!(message["state"], "dispatched");
    assert_eq!(message["turn_id"], turn);
    let items = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let live = h.app.live_output().snapshot(&session, &turn).unwrap();
            let items = serde_json::to_value(live.items).unwrap();
            if items
                .as_array()
                .unwrap()
                .iter()
                .any(|i| i["text"] == "前後[OTHER]")
            {
                break items;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        items
            .as_array()
            .unwrap()
            .iter()
            .filter(|i| i["kind"] == "user_input")
            .count(),
        1
    );
    expect_pending_history(&h, &session).await;
    h.release("release-1");
    h.wait_turn(&session, &turn, "completed").await;
    let history = h.history(&session).await.unwrap();
    assert!(history.items.iter().any(|i| i.turn_id.as_deref() == Some(&turn) && i.item.content_preview == "前後[OTHER]"));
    assert_eq!(h.requests("session/prompt"), 1);
    assert_eq!(h.requests("session/cancel"), 0);
    h.stop().await;
}

#[tokio::test]
async fn kiro_queued_without_matching_injection_is_undelivered() {
    for timeout in [false, true] {
        let mode = if timeout { "inject" } else { "queued" };
        let h = Harness::new(&[("ACP_KIRO", "1"), ("ACP_KIRO_STEER", mode)]).await;
        let session = h.create("kiro").await.unwrap();
        let turn = start(&h, &session).await;
        let mut delivery = Box::pin(submit(&h, &session, "steer"));
        tokio::select! {
            _ = &mut delivery => panic!("unmatched injection completed steering"),
            _ = wait_fixture_response(&h, "steer-queued-barrier") => {}
        }
        if timeout {
            tokio::time::pause();
            tokio::time::advance(Duration::from_secs(31)).await;
            tokio::time::resume();
        } else {
            h.release("release-1");
        }
        let receipt = delivery.await;
        assert_eq!(receipt["state"], if timeout { "unknown" } else { "failed" });
        assert!(receipt["turn_id"].is_null());
        if timeout {
            h.release("inject");
            wait_fixture_response(&h, "steer-barrier").await;
            let items =
                serde_json::to_value(h.app.live_output().snapshot(&session, &turn).unwrap().items)
                    .unwrap();
            assert!(
                !items
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|item| item["kind"] == "user_input")
            );
            h.release("release-1");
        }
        h.wait_turn(&session, &turn, "completed").await;
        assert_eq!(h.requests("session/prompt"), 1);
        assert_eq!(h.requests("session/cancel"), 0);
        h.stop().await;
    }
}

#[tokio::test]
async fn kiro_hides_the_rest_of_a_note_after_controller_reattachment() {
    let mut h = Harness::new(&[("ACP_KIRO", "1"), ("ACP_KIRO_STEER", "reattach")]).await;
    let session = h.create("kiro").await.unwrap();
    let turn = start(&h, &session).await;
    h.release("inject");
    assert_eq!(submit(&h, &session, "steer").await["turn_id"], turn);
    wait_fixture_response(&h, "note-barrier").await;
    h.restart().await;
    tokio::time::timeout(Duration::from_secs(10), async {
        while !h
            .app
            .queries()
            .get_session(&session)
            .await
            .unwrap()
            .unwrap()
            .capabilities
            .steer
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    h.release("continue-note");
    wait_fixture_response(&h, "steer-barrier").await;
    let items =
        serde_json::to_value(h.app.live_output().snapshot(&session, &turn).unwrap().items).unwrap();
    assert!(
        items
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["text"] == "後[OTHER]")
    );
    h.release("release-1");
    h.wait_turn(&session, &turn, "completed").await;
    h.stop().await;
}
