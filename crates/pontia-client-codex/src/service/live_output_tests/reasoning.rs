use super::*;

fn summary(item: &str, index: u64, text: &str) -> Value {
    json!({"method":"item/reasoning/summaryTextDelta","params":{"threadId":"thread","turnId":"native-turn","itemId":item,"summaryIndex":index,"delta":text}})
}

fn reasoning(item: &str, index: u64, value: &str) -> LiveOutputItem {
    LiveOutputItem::Reasoning {
        item_id: format!("reasoning:{item}:{index}"),
        text: value.into(),
        truncated: false,
    }
}

#[tokio::test]
async fn summary_parts_stream_as_separate_items_in_order_with_text_and_tools() {
    let fixture = Fixture::new(turn(json!([])), Vec::new()).await;
    let initial = fixture.wait_snapshot(|_| true).await;
    for notification in [
        summary("r1", 0, "Reading"),
        summary("r1", 0, " config"),
        summary("r1", 1, "Planning"),
        json!({"method":"item/started","params":{"threadId":"thread","turnId":"native-turn","item":{"type":"commandExecution","id":"cmd","command":"ls","status":"inProgress"}}}),
        summary("r2", 0, "Checking output"),
        // Raw reasoning content is not shown, matching the native timeline.
        json!({"method":"item/reasoning/textDelta","params":{"threadId":"thread","turnId":"native-turn","itemId":"r2","contentIndex":0,"delta":"raw"}}),
        delta("thread", "native-turn", "answer", json!("done")),
    ] {
        fixture.notify(notification).await;
    }
    let snapshot = fixture
        .wait_snapshot(|s| s.items.last() == Some(&text("answer", "done")))
        .await;
    let ids: Vec<_> = snapshot.items.iter().map(item_summary).collect();
    assert_eq!(
        ids,
        vec![
            "reasoning:Reading config",
            "reasoning:Planning",
            "tool:cmd",
            "reasoning:Checking output",
            "text:done",
        ]
    );
    assert_eq!(snapshot.identity, initial.identity);
    fixture.shutdown().await;
}

#[tokio::test]
async fn native_summary_recovery_covers_queued_deltas_while_new_parts_keep_streaming() {
    let fixture = Fixture::new(
        turn(json!([{"type":"reasoning","id":"r1","summary":["Reading config"]}])),
        vec![
            summary("r1", 0, "Reading config"),
            summary("r1", 1, "Planning"),
        ],
    )
    .await;
    let snapshot = fixture.wait_snapshot(|s| s.items.len() == 2).await;
    assert_eq!(
        snapshot.items,
        vec![
            reasoning("r1", 0, "Reading config"),
            reasoning("r1", 1, "Planning")
        ]
    );
    fixture.notify(summary("r1", 1, " next")).await;
    let updated = fixture
        .wait_snapshot(|s| s.sequence > snapshot.sequence)
        .await;
    assert_eq!(
        updated.items,
        vec![
            reasoning("r1", 0, "Reading config"),
            reasoning("r1", 1, "Planning next")
        ]
    );
    fixture.shutdown().await;
}

#[tokio::test]
async fn lost_snapshot_is_rebuilt_from_native_summary_before_the_next_part() {
    let fixture = Fixture::new(turn(json!([])), Vec::new()).await;
    let initial = fixture.wait_snapshot(|_| true).await;
    *fixture.server_state.turns.lock().await = vec![turn(
        json!([{"type":"reasoning","id":"r1","summary":["first","second"]}]),
    )];
    fixture
        .app
        .live_output()
        .discard_turn(&fixture.session, &fixture.turn_id().await);
    fixture.notify(summary("r1", 2, "third")).await;
    let snapshot = fixture.wait_snapshot(|s| s.items.len() == 3).await;
    assert_eq!(snapshot.identity, initial.identity);
    assert_eq!(
        snapshot.items,
        vec![
            reasoning("r1", 0, "first"),
            reasoning("r1", 1, "second"),
            reasoning("r1", 2, "third")
        ]
    );
    fixture.shutdown().await;
}

#[tokio::test]
async fn untrusted_native_history_stops_reasoning_but_tools_keep_streaming() {
    let fixture = Fixture::new(turn(json!([])), Vec::new()).await;
    let mut subscriber = fixture
        .app
        .live_output()
        .subscribe_session(&fixture.session);
    *fixture.server_state.turns.lock().await = vec![turn(
        json!([{"type":"reasoning","id":"item-1","summary":["legacy"]}]),
    )];
    let runtime = CodexRuntime::install_for_test(fixture.root.path(), fixture.socket.clone())
        .await
        .unwrap();
    assert!(matches!(
        tokio::time::timeout(Duration::from_secs(3), subscriber.recv())
            .await
            .unwrap()
            .unwrap(),
        LiveOutputStreamEvent::Closed { .. }
    ));
    tokio::time::timeout(Duration::from_secs(6), async {
        while runtime.subscription(&fixture.session).await
            != Some(crate::runtime::SubscriptionState::Available)
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    fixture.notify(summary("item-2", 0, "hidden")).await;
    fixture
        .notify(json!({"method":"item/started","params":{"threadId":"thread","turnId":"native-turn","item":{"type":"commandExecution","id":"cmd","command":"ls","status":"inProgress"}}}))
        .await;
    fixture.notify(summary("item-2", 0, " still hidden")).await;
    // Notifications are processed in order; a later tool marks the delta as handled.
    fixture
        .notify(json!({"method":"item/started","params":{"threadId":"thread","turnId":"native-turn","item":{"type":"commandExecution","id":"marker","command":"pwd","status":"inProgress"}}}))
        .await;
    let snapshot = fixture.wait_snapshot(|s| s.items.len() >= 2).await;
    let ids: Vec<_> = snapshot.items.iter().map(item_summary).collect();
    assert_eq!(ids, vec!["tool:cmd", "tool:marker"]);
    fixture.shutdown().await;
}

fn item_summary(item: &LiveOutputItem) -> String {
    match item {
        LiveOutputItem::AssistantText { text, .. } => format!("text:{text}"),
        LiveOutputItem::Reasoning { text, .. } => format!("reasoning:{text}"),
        LiveOutputItem::UserInput { text, .. } => format!("input:{text}"),
        LiveOutputItem::ToolCall { call_id, .. } => format!("tool:{call_id}"),
    }
}

#[tokio::test]
async fn consumed_steering_streams_at_its_position_once_and_survives_recovery() {
    let fixture = Fixture::new(turn(json!([])), Vec::new()).await;
    fixture.wait_snapshot(|_| true).await;
    let steer = json!({"type":"userMessage","id":"steer","content":[{"type":"text","text":"change direction"}]});
    fixture
        .notify(delta("thread", "native-turn", "a1", json!("before")))
        .await;
    fixture.server_state.turns.lock().await[0]["items"]
        .as_array_mut()
        .unwrap()
        .push(steer.clone());
    for method in ["item/started", "item/completed"] {
        fixture
            .notify(json!({"method":method,"params":{"threadId":"thread","turnId":"native-turn","item":steer}}))
            .await;
    }
    fixture
        .notify(delta("thread", "native-turn", "a2", json!("after")))
        .await;
    let snapshot = fixture
        .wait_snapshot(|s| s.items.last() == Some(&text("a2", "after")))
        .await;
    let ids: Vec<_> = snapshot.items.iter().map(item_summary).collect();
    assert_eq!(
        ids,
        vec!["text:before", "input:change direction", "text:after"]
    );

    // The Turn's own input never becomes a live item.
    let first =
        json!({"type":"userMessage","id":"user-item","content":[{"type":"text","text":"hello"}]});
    fixture
        .notify(json!({"method":"item/completed","params":{"threadId":"thread","turnId":"native-turn","item":first}}))
        .await;
    fixture
        .notify(delta("thread", "native-turn", "a2", json!("!")))
        .await;
    let snapshot = fixture
        .wait_snapshot(|s| s.items.last() == Some(&text("a2", "after!")))
        .await;
    assert_eq!(
        snapshot.items.iter().map(item_summary).collect::<Vec<_>>(),
        vec!["text:before", "input:change direction", "text:after!"]
    );
    fixture.shutdown().await;
}

#[tokio::test]
async fn client_identity_excludes_turn_input_but_keeps_identical_steering() {
    let mut initial = turn(json!([]));
    initial["items"][0]["id"] = json!("item-1");
    initial["items"][0]["clientId"] = json!("submitted-input");
    let fixture = Fixture::new(initial, Vec::new()).await;
    let first = json!({"type":"userMessage","id":"live-input","clientId":"submitted-input","content":[{"type":"text","text":"hello"}]});
    for method in ["item/started", "item/completed"] {
        fixture
            .notify(json!({"method":method,"params":{"threadId":"thread","turnId":"native-turn","item":first}}))
            .await;
    }
    fixture
        .notify(delta("thread", "native-turn", "a1", json!("before")))
        .await;
    let snapshot = fixture
        .wait_snapshot(|s| s.items.last() == Some(&text("a1", "before")))
        .await;
    assert_eq!(snapshot.items, vec![text("a1", "before")]);

    for (history_id, live_id, client_id) in [
        ("item-3", "live-steer-1", "submitted-steer-1"),
        ("item-4", "live-steer-2", "submitted-steer-2"),
    ] {
        let mut steer = first.clone();
        steer["id"] = json!(history_id);
        steer["clientId"] = json!(client_id);
        fixture.server_state.turns.lock().await[0]["items"]
            .as_array_mut()
            .unwrap()
            .push(steer.clone());
        steer["id"] = json!(live_id);
        for method in ["item/started", "item/completed"] {
            fixture
                .notify(json!({"method":method,"params":{"threadId":"thread","turnId":"native-turn","item":steer}}))
                .await;
        }
    }
    fixture
        .notify(delta("thread", "native-turn", "a2", json!("after")))
        .await;
    let snapshot = fixture
        .wait_snapshot(|s| s.items.last() == Some(&text("a2", "after")))
        .await;
    assert_eq!(
        snapshot.items.iter().map(item_summary).collect::<Vec<_>>(),
        vec!["text:before", "input:hello", "input:hello", "text:after"]
    );
    fixture.shutdown().await;
}

#[tokio::test]
async fn recovered_steering_with_client_identity_is_not_repeated_by_notifications() {
    let mut initial = turn(json!([
        {"type":"agentMessage","id":"a1","text":"before"},
        {"type":"userMessage","id":"item-3","clientId":"submitted-steer","content":[{"type":"text","text":"hello"}]}
    ]));
    initial["items"][0]["id"] = json!("item-1");
    initial["items"][0]["clientId"] = json!("submitted-input");
    let fixture = Fixture::new(initial, Vec::new()).await;
    let snapshot = fixture.wait_snapshot(|_| true).await;
    assert_eq!(
        snapshot.items.iter().map(item_summary).collect::<Vec<_>>(),
        vec!["text:before", "input:hello"]
    );
    for client_id in ["submitted-input", "submitted-steer"] {
        for method in ["item/started", "item/completed"] {
            fixture
                .notify(json!({"method":method,"params":{"threadId":"thread","turnId":"native-turn","item":{"type":"userMessage","id":format!("live-{client_id}"),"clientId":client_id,"content":[{"type":"text","text":"hello"}]}}}))
                .await;
        }
    }
    fixture
        .notify(delta("thread", "native-turn", "a2", json!("after")))
        .await;
    let snapshot = fixture
        .wait_snapshot(|s| s.items.last() == Some(&text("a2", "after")))
        .await;
    assert_eq!(
        snapshot.items.iter().map(item_summary).collect::<Vec<_>>(),
        vec!["text:before", "input:hello", "text:after"]
    );
    fixture.shutdown().await;
}

#[tokio::test]
async fn native_recovery_restores_steering_after_the_turn_input() {
    let fixture = Fixture::new(
        turn(json!([
            {"type":"agentMessage","id":"a1","text":"before"},
            {"type":"userMessage","id":"steer","content":[{"type":"text","text":"change direction"}]},
            {"type":"agentMessage","id":"a2","text":"after"}
        ])),
        Vec::new(),
    )
    .await;
    let snapshot = fixture.wait_snapshot(|s| s.items.len() == 3).await;
    assert_eq!(
        snapshot.items.iter().map(item_summary).collect::<Vec<_>>(),
        vec!["text:before", "input:change direction", "text:after"]
    );
    fixture.shutdown().await;
}

#[tokio::test]
async fn direct_inputs_without_client_ids_survive_recovery_and_live_steering() {
    let history_input = |id: &str| {
        json!({
            "type":"userMessage", "id":id, "clientId":null,
            "content":[{"type":"text","text":"same input"}]
        })
    };
    let mut initial = turn(json!([
        {"type":"agentMessage","id":"before","text":"before"},
        history_input("item-3"),
    ]));
    initial["items"][0] = history_input("item-1");
    let fixture = Fixture::new(initial, Vec::new()).await;
    let recovered = fixture.wait_snapshot(|_| true).await;
    assert_eq!(
        recovered.items.iter().map(item_summary).collect::<Vec<_>>(),
        ["text:before", "input:same input"]
    );

    fixture.server_state.turns.lock().await[0]["items"]
        .as_array_mut()
        .unwrap()
        .extend([history_input("item-4"), history_input("item-6")]);
    // A full history read can already see both steers while the observer drains
    // the notifications separated by an assistant delta.
    for (id, reply) in [("uuid-a", "between"), ("uuid-b", "next")] {
        for method in ["item/started", "item/completed"] {
            fixture
                .notify(json!({"method":method,"params":{
                    "threadId":"thread","turnId":"native-turn","item":history_input(id)
                }}))
                .await;
        }
        fixture
            .notify(delta("thread", "native-turn", reply, json!(reply)))
            .await;
    }
    fixture
        .notify(delta("thread", "native-turn", "after", json!("after")))
        .await;
    let snapshot = fixture
        .wait_snapshot(|s| s.items.last() == Some(&text("after", "after")))
        .await;
    assert_eq!(
        snapshot.items.iter().map(item_summary).collect::<Vec<_>>(),
        [
            "text:before",
            "input:same input",
            "input:same input",
            "text:between",
            "input:same input",
            "text:next",
            "text:after"
        ]
    );
    fixture.shutdown().await;
}
