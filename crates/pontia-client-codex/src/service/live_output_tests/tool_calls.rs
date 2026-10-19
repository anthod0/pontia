use super::*;
use pontia_application::LiveOutputUpdate;

fn item_notification(method: &str, item: Value) -> Value {
    json!({"method":method,"params":{"threadId":"thread","turnId":"native-turn","item":item}})
}

fn call(id: &str, name: &str, arguments: Value) -> LiveOutputItem {
    LiveOutputItem::ToolCall {
        item_id: id.into(),
        call_id: id.into(),
        tool_name: name.into(),
        arguments,
        managed_tool_use: None,
    }
}

fn cases() -> Vec<(Value, LiveOutputItem)> {
    vec![
        (
            json!({"type":"sleep","id":"sleep","durationMs":2000}),
            call("sleep", "clock.sleep", json!({"durationMs":2000})),
        ),
        (
            json!({"type":"imageView","id":"image","path":"/workspace/image.png"}),
            call("image", "imageView", json!({"path":"/workspace/image.png"})),
        ),
        (
            json!({"type":"webSearch","id":"search","query":"rust","action":{"type":"search","query":"rust","queries":["rust","cargo"]},"results":[{"title":"result"}]}),
            call(
                "search",
                "webSearch",
                json!({"query":"rust","action":{"type":"search","query":"rust","queries":["rust","cargo"]}}),
            ),
        ),
        (
            json!({"type":"webSearch","id":"open","query":"","action":{"type":"openPage","url":"https://example.com"},"results":[]}),
            call(
                "open",
                "webSearch",
                json!({"action":{"type":"openPage","url":"https://example.com"}}),
            ),
        ),
        (
            json!({"type":"webSearch","id":"find","query":"","action":{"type":"findInPage","url":"https://example.com","pattern":"rust"},"results":[]}),
            call(
                "find",
                "webSearch",
                json!({"action":{"type":"findInPage","url":"https://example.com","pattern":"rust"}}),
            ),
        ),
        (
            json!({"type":"webSearch","id":"query-only","query":"rust","action":null,"results":[]}),
            call("query-only", "webSearch", json!({"query":"rust"})),
        ),
        (
            json!({"type":"commandExecution","id":"cmd","command":"printf hello","cwd":"/workspace","processId":"42","source":"agent","status":"completed","commandActions":[],"aggregatedOutput":"hello","exitCode":0,"durationMs":10}),
            call(
                "cmd",
                "commandExecution",
                json!({"command":"printf hello","cwd":"/workspace"}),
            ),
        ),
        (
            json!({"type":"fileChange","id":"patch","changes":[{"path":"a.txt","kind":{"type":"add"},"diff":"hello"},{"path":"b.txt","kind":{"type":"update","move_path":"c.txt"},"diff":"-old\n+new"},{"path":"d.txt","kind":{"type":"delete"},"diff":"removed"}],"status":"completed"}),
            call(
                "patch",
                "fileChange",
                json!({"changes":[{"path":"a.txt","kind":{"type":"add"},"diff":"hello"},{"path":"b.txt","kind":{"type":"update","move_path":"c.txt"},"diff":"-old\n+new"},{"path":"d.txt","kind":{"type":"delete"},"diff":"removed"}]}),
            ),
        ),
        (
            json!({"type":"mcpToolCall","id":"mcp","server":"docs","tool":"search","arguments":{"query":"rust"},"status":"failed","result":{"content":"result"},"error":{"message":"error"},"durationMs":10}),
            call("mcp", "docs/search", json!({"query":"rust"})),
        ),
        (
            json!({"type":"dynamicToolCall","id":"dynamic","namespace":"custom","tool":"lookup","arguments":{"key":3},"status":"completed","contentItems":[{"type":"inputText","text":"result"}],"success":true}),
            call("dynamic", "custom/lookup", json!({"key":3})),
        ),
        (
            json!({"type":"dynamicToolCall","id":"dynamic-null","namespace":null,"tool":"ping","arguments":null,"status":"completed","success":true}),
            call("dynamic-null", "ping", Value::Null),
        ),
        (
            json!({"type":"collabAgentToolCall","id":"spawn","tool":"spawnAgent","senderThreadId":"thread","receiverThreadIds":["created-agent"],"prompt":"inspect","model":"model","reasoningEffort":"high","status":"completed","agentsStates":{"created-agent":{"status":"running"}}}),
            call(
                "spawn",
                "spawnAgent",
                json!({"prompt":"inspect","model":"model","reasoningEffort":"high"}),
            ),
        ),
        (
            json!({"type":"collabAgentToolCall","id":"send","tool":"sendInput","senderThreadId":"thread","receiverThreadIds":["agent"],"prompt":"continue","model":null,"reasoningEffort":null,"status":"completed","agentsStates":{"agent":{"status":"completed","message":"result"}}}),
            call(
                "send",
                "sendInput",
                json!({"receiverThreadIds":["agent"],"prompt":"continue"}),
            ),
        ),
    ]
}

#[tokio::test]
async fn publishes_each_supported_invocation_before_turn_completion_without_results() {
    let fixture = Fixture::new(turn(json!([])), Vec::new()).await;
    let mut subscriber = fixture
        .app
        .live_output()
        .subscribe_session(&fixture.session);
    let mut expected = Vec::new();
    for (native, item) in cases() {
        fixture
            .notify(item_notification("item/started", native))
            .await;
        let event = tokio::time::timeout(Duration::from_secs(3), subscriber.recv())
            .await
            .unwrap()
            .unwrap();
        let LiveOutputStreamEvent::Updates { updates, .. } = event else {
            panic!("expected live update, got {event:?}");
        };
        let LiveOutputItem::ToolCall {
            item_id,
            call_id,
            tool_name,
            arguments,
            managed_tool_use,
        } = &item
        else {
            unreachable!()
        };
        assert_eq!(
            updates,
            vec![LiveOutputUpdate::ToolCall {
                item_id: item_id.clone(),
                call_id: call_id.clone(),
                tool_name: tool_name.clone(),
                arguments: arguments.clone(),
                managed_tool_use: managed_tool_use.clone(),
            }]
        );
        expected.push(item);
    }
    let snapshot = fixture
        .wait_snapshot(|s| s.items.len() == expected.len())
        .await;
    assert_eq!(snapshot.items, expected);
    let state: String = sqlx::query_scalar("SELECT state FROM turns WHERE turn_id=?")
        .bind(fixture.turn_id().await)
        .fetch_one(&fixture.app.db())
        .await
        .unwrap();
    assert_eq!(state, "running");
    fixture.shutdown().await;
}

#[tokio::test]
async fn starts_completions_and_replays_add_each_call_only_once() {
    let fixture = Fixture::new(turn(json!([])), Vec::new()).await;
    let initial = fixture.wait_snapshot(|_| true).await;
    let native = json!({"type":"dynamicToolCall","id":"one","tool":"lookup","arguments":{"key":1}});
    for method in [
        "item/started",
        "item/started",
        "item/completed",
        "item/completed",
    ] {
        let mut notification = native.clone();
        if method == "item/completed" {
            notification["arguments"] = json!({"key":2});
            notification["contentItems"] = json!([{"text":"result"}]);
        }
        fixture
            .notify(item_notification(method, notification))
            .await;
    }
    let completed_only = json!({"type":"commandExecution","id":"two","command":"pwd","status":"completed","aggregatedOutput":"/workspace"});
    for _ in 0..2 {
        fixture
            .notify(item_notification("item/completed", completed_only.clone()))
            .await;
    }
    fixture
        .notify(delta("thread", "native-turn", "text", json!("after")))
        .await;
    let snapshot = fixture
        .wait_snapshot(|s| s.items.last() == Some(&text("text", "after")))
        .await;
    assert_eq!(snapshot.sequence, initial.sequence + 3);
    assert_eq!(
        snapshot.items,
        vec![
            call("one", "lookup", json!({"key":1})),
            call("two", "commandExecution", json!({"command":"pwd"})),
            text("text", "after"),
        ]
    );
    fixture.shutdown().await;
}

#[tokio::test]
async fn mixed_snapshot_restores_all_supported_calls_and_deduplicates_overlapping_notifications() {
    let cases = cases();
    let mut native = vec![json!({"type":"agentMessage","id":"text","text":"before"})];
    native.extend(cases.iter().map(|(native, _)| native.clone()));
    let mut queued = vec![delta("thread", "native-turn", "text", json!("before"))];
    for (item, _) in &cases {
        queued.push(item_notification("item/started", item.clone()));
        queued.push(item_notification("item/completed", item.clone()));
    }
    let fixture = Fixture::new(turn(json!(native)), queued).await;
    let initial = fixture.wait_snapshot(|_| true).await;
    // Replays arriving after the recovery boundary must also be ignored.
    for (item, _) in &cases {
        fixture
            .notify(item_notification("item/completed", item.clone()))
            .await;
    }
    fixture
        .notify(delta("thread", "native-turn", "text", json!(" after")))
        .await;
    fixture
        .notify(item_notification(
            "item/started",
            json!({"type":"dynamicToolCall","id":"new","tool":"next","arguments":{}}),
        ))
        .await;
    let snapshot = fixture
        .wait_snapshot(|s| s.items.len() == cases.len() + 2)
        .await;
    let mut expected = vec![text("text", "before after")];
    expected.extend(cases.into_iter().map(|(_, item)| item));
    expected.push(call("new", "next", json!({})));
    assert_eq!(snapshot.items, expected);
    assert_eq!(snapshot.sequence, initial.sequence + 2);
    fixture.shutdown().await;
}

#[tokio::test]
async fn reconnect_recovers_calls_and_keeps_text_and_new_calls_streaming() {
    let fixture = Fixture::new(turn(json!([])), Vec::new()).await;
    let native = json!({"type":"commandExecution","id":"cmd","command":"pwd"});
    fixture
        .notify(item_notification("item/started", native.clone()))
        .await;
    let original = fixture.wait_snapshot(|s| s.items.len() == 1).await;
    *fixture.server_state.turns.lock().await = vec![turn(json!([
        native, {"type":"agentMessage","id":"text","text":"restored"}
    ]))];
    *fixture.server_state.before_snapshot.lock().await =
        vec![item_notification("item/completed", native.clone())];
    CodexRuntime::install_for_test(fixture.root.path(), fixture.socket.clone())
        .await
        .unwrap();
    fixture.wait_snapshot(|s| s.items.len() == 2).await;
    fixture
        .notify(item_notification("item/completed", native))
        .await;
    fixture
        .notify(delta("thread", "native-turn", "text", json!(" after")))
        .await;
    fixture
        .notify(item_notification(
            "item/started",
            json!({"type":"dynamicToolCall","id":"new","tool":"ping","arguments":{}}),
        ))
        .await;
    let snapshot = fixture.wait_snapshot(|s| s.items.len() == 3).await;
    assert_eq!(snapshot.identity, original.identity);
    assert_eq!(
        snapshot.items,
        vec![
            call("cmd", "commandExecution", json!({"command":"pwd"})),
            text("text", "restored after"),
            call("new", "ping", json!({}))
        ]
    );
    fixture.shutdown().await;
}

#[tokio::test]
async fn legacy_text_ids_do_not_prevent_recovering_stable_tool_ids() {
    let command = json!({"type":"commandExecution","id":"cmd","command":"pwd"});
    let fixture = Fixture::new(
        turn(json!([
            {"type":"agentMessage","id":"item-2","text":"unsafe baseline"}, command
        ])),
        vec![item_notification("item/started", command.clone())],
    )
    .await;
    let initial = fixture.wait_snapshot(|_| true).await;
    assert_eq!(
        initial.items,
        vec![call("cmd", "commandExecution", json!({"command":"pwd"}))]
    );
    fixture
        .notify(delta(
            "thread",
            "native-turn",
            "native-message",
            json!("unsafe baseline"),
        ))
        .await;
    fixture
        .notify(item_notification("item/completed", command.clone()))
        .await;
    fixture
        .notify(item_notification(
            "item/started",
            json!({"type":"dynamicToolCall","id":"new","tool":"ping","arguments":{}}),
        ))
        .await;
    let snapshot = fixture.wait_snapshot(|s| s.items.len() == 2).await;
    assert_eq!(snapshot.sequence, initial.sequence + 1);
    assert_eq!(
        snapshot.items,
        vec![
            call("cmd", "commandExecution", json!({"command":"pwd"})),
            call("new", "ping", json!({}))
        ]
    );

    // The delta that triggers native recovery must also respect the unsafe baseline.
    fixture
        .app
        .live_output()
        .discard_turn(&fixture.session, &fixture.turn_id().await);
    fixture
        .notify(delta(
            "thread",
            "native-turn",
            "native-message",
            json!("unsafe baseline"),
        ))
        .await;
    fixture
        .notify(item_notification(
            "item/started",
            json!({"type":"dynamicToolCall","id":"next","tool":"pong","arguments":{}}),
        ))
        .await;
    let restored = fixture
        .wait_snapshot(|s| s.items.last() == Some(&call("next", "pong", json!({}))))
        .await;
    assert_eq!(
        restored.items,
        vec![
            call("cmd", "commandExecution", json!({"command":"pwd"})),
            call("next", "pong", json!({}))
        ]
    );
    fixture.shutdown().await;
}

#[tokio::test]
async fn live_call_absent_from_history_still_streams_when_text_cannot_be_recovered() {
    let mut summary = turn(json!([]));
    summary["itemsView"] = json!("summary");
    let mut missing = turn(json!([]));
    missing.as_object_mut().unwrap().remove("items");
    for native in [
        turn(json!([{"type":"agentMessage","id":"item-2","text":"unsafe"}])),
        summary,
        missing,
    ] {
        let fixture = Fixture::new(turn(json!([])), Vec::new()).await;
        *fixture.server_state.turns.lock().await = vec![native];
        fixture
            .app
            .live_output()
            .discard_turn(&fixture.session, &fixture.turn_id().await);
        let command = json!({"type":"commandExecution","id":"unfinished","command":"sleep 10"});
        fixture
            .notify(item_notification("item/started", command.clone()))
            .await;
        let first = fixture.wait_snapshot(|s| s.items.len() == 1).await;
        fixture
            .notify(item_notification("item/completed", command))
            .await;
        fixture
            .notify(delta(
                "thread",
                "native-turn",
                "native-msg",
                json!("unsafe"),
            ))
            .await;
        fixture
            .notify(item_notification(
                "item/started",
                json!({"type":"sleep","id":"next","durationMs":1}),
            ))
            .await;
        let snapshot = fixture.wait_snapshot(|s| s.items.len() == 2).await;
        assert_eq!(snapshot.sequence, first.sequence + 1);
        assert_eq!(
            snapshot.items,
            vec![
                call(
                    "unfinished",
                    "commandExecution",
                    json!({"command":"sleep 10"})
                ),
                call("next", "clock.sleep", json!({"durationMs":1}))
            ]
        );
        fixture.shutdown().await;
    }
}

#[tokio::test]
async fn unsupported_outputs_and_incomplete_calls_do_not_block_text_or_later_discovery() {
    let ignored = vec![
        json!({"type":"reasoning","id":"reason","summary":["private"]}),
        json!({"type":"plan","id":"plan","text":"plan"}),
        json!({"type":"functionCallOutput","id":"output","name":"tool","output":"result"}),
        json!({"type":"subAgentActivity","id":"activity","kind":"spawn","agentThreadId":"agent"}),
        json!({"type":"unknownTool","id":"unknown","arguments":{"key":1}}),
        json!({"type":"imageGeneration","id":"generated","revisedPrompt":"output prompt","result":"base64 result","savedPath":"/workspace/result.png","status":"completed"}),
        json!({"type":"webSearch","id":"pending-search","query":"","action":null,"results":[]}),
        json!({"type":"commandExecution","id":"missing-input","aggregatedOutput":"output"}),
        json!({"type":"mcpToolCall","id":"missing-tool","server":"server","arguments":{}}),
        json!({"type":"dynamicToolCall","id":"missing-args","tool":"tool","contentItems":[]}),
        json!({"type":"commandExecution","id":"item-42","command":"legacy id"}),
    ];
    let fixture = Fixture::new(turn(json!(ignored)), Vec::new()).await;
    let initial = fixture.wait_snapshot(|_| true).await;
    assert!(initial.items.is_empty());
    for item in ignored {
        fixture
            .notify(item_notification("item/started", item))
            .await;
    }
    for method in [
        "item/commandExecution/outputDelta",
        "item/fileChange/outputDelta",
        "item/mcpToolCall/progress",
    ] {
        fixture.notify(json!({"method":method,"params":{"threadId":"thread","turnId":"native-turn","itemId":"output","delta":"output","message":"progress"}})).await;
    }
    fixture.notify(item_notification("item/completed", json!({"type":"commandExecution","id":"missing-input","command":"pwd","aggregatedOutput":"result"}))).await;
    fixture
        .notify(delta(
            "thread",
            "native-turn",
            "text",
            json!("still streaming"),
        ))
        .await;
    let snapshot = fixture.wait_snapshot(|s| s.items.len() == 2).await;
    assert_eq!(snapshot.sequence, initial.sequence + 2);
    assert_eq!(
        snapshot.items,
        vec![
            call(
                "missing-input",
                "commandExecution",
                json!({"command":"pwd"})
            ),
            text("text", "still streaming")
        ]
    );
    fixture.shutdown().await;
}
