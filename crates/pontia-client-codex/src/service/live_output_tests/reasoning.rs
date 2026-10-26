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
        LiveOutputItem::ToolCall { call_id, .. } => format!("tool:{call_id}"),
    }
}
