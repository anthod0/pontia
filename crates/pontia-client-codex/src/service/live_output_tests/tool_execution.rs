use super::*;

fn item(method: &str, value: Value) -> Value {
    json!({"method":method,"params":{"threadId":"thread","turnId":"native-turn","item":value}})
}
fn output(id: &str, text: &str) -> Value {
    json!({"method":"item/commandExecution/outputDelta","params":{"threadId":"thread","turnId":"native-turn","itemId":id,"delta":text}})
}
async fn settled(fixture: &Fixture) -> Value {
    fixture
        .notify(delta("thread", "native-turn", "marker", json!("after")))
        .await;
    let snapshot = fixture
        .wait_snapshot(|s| s.items.last() == Some(&text("marker", "after")))
        .await;
    serde_json::to_value(snapshot.items).unwrap()
}

#[tokio::test]
async fn command_outputs_and_results_stay_with_parallel_calls_and_do_not_end_turn() {
    let fixture = Fixture::new(turn(json!([])), vec![]).await;
    for id in ["one", "two"] {
        fixture
            .notify(item(
                "item/started",
                json!({"type":"commandExecution","id":id,"command":"test","status":"inProgress"}),
            ))
            .await;
    }
    fixture.notify(output("one", "a")).await;
    fixture.notify(output("two", "failure")).await;
    fixture.notify(output("one", "b")).await;
    let one = json!({"type":"commandExecution","id":"one","command":"test","status":"completed","exitCode":0,"aggregatedOutput":"ab"});
    let two = json!({"type":"commandExecution","id":"two","command":"test","status":"completed","exitCode":2,"aggregatedOutput":"failure"});
    for value in [one.clone(), two, one] {
        fixture.notify(item("item/completed", value)).await;
    }
    fixture
        .notify(item(
            "item/started",
            json!({"type":"commandExecution","id":"one","command":"test","status":"inProgress"}),
        ))
        .await;
    let value = settled(&fixture).await;
    assert_eq!(value.as_array().unwrap().len(), 3);
    assert_eq!(value[0]["output"]["text"], "ab");
    assert_eq!(value[0]["result"]["content"], "ab");
    assert_eq!(
        value[0]["status"],
        json!({"phase":"ended","outcome":"succeeded"})
    );
    assert_eq!(value[1]["output"]["text"], "failure");
    assert_eq!(value[1]["status"]["outcome"], "failed");
    let state: String = sqlx::query_scalar("SELECT state FROM turns WHERE turn_id=?")
        .bind(fixture.turn_id().await)
        .fetch_one(&fixture.app.db())
        .await
        .unwrap();
    assert_eq!(state, "running");
    fixture.shutdown().await;
}

#[tokio::test]
async fn initial_command_aggregate_precedes_deltas_and_replayed_start_cannot_replace_them() {
    let fixture = Fixture::new(turn(json!([])), vec![]).await;
    let start = json!({"type":"commandExecution","id":"cmd","command":"test","status":"inProgress","aggregatedOutput":"first\n"});
    fixture.notify(item("item/started", start.clone())).await;
    fixture.notify(output("cmd", "second\n")).await;
    fixture.notify(item("item/started", start)).await;
    let value = settled(&fixture).await;
    assert_eq!(value[0]["output"]["text"], "first\nsecond\n");
    fixture.shutdown().await;
}

#[tokio::test]
async fn completion_only_discovers_results_without_inventing_success() {
    let fixture = Fixture::new(turn(json!([])), vec![]).await;
    for native in [
        json!({"type":"commandExecution","id":"unknown","command":"test","status":"completed"}),
        json!({"type":"commandExecution","id":"denied","command":"test","status":"declined"}),
        json!({"type":"collabAgentToolCall","id":"interrupted","tool":"wait","receiverThreadIds":["agent"],"status":"interrupted"}),
        json!({"type":"mcpToolCall","id":"mcp","server":"docs","tool":"search","arguments":{},"status":"failed","error":{"message":"denied by server"}}),
        json!({"type":"dynamicToolCall","id":"dynamic","tool":"custom","arguments":{},"status":"completed","success":true,"contentItems":[{"type":"inputText","text":"ok"}]}),
    ] {
        fixture.notify(item("item/completed", native)).await;
    }
    let value = settled(&fixture).await;
    assert_eq!(value[0]["status"], json!({"phase":"ended"}));
    assert!(value[0].get("result").is_none());
    assert_eq!(value[1]["status"]["outcome"], "rejected");
    assert_eq!(value[2]["status"]["outcome"], "interrupted");
    assert_eq!(
        value[3]["result"]["error"],
        json!({"message":"denied by server"})
    );
    assert_eq!(value[4]["status"]["outcome"], "succeeded");
    assert_eq!(
        value[4]["result"]["content"],
        json!([{"type":"inputText","text":"ok"}])
    );
    fixture.shutdown().await;
}

#[tokio::test]
async fn recovery_reserves_later_call_inputs_before_truncating_large_results() {
    let fixture = Fixture::new(turn(json!([
        {"type":"dynamicToolCall","id":"large","tool":"custom","arguments":{},"status":"completed","success":true,"contentItems":[{"type":"inputText","text":"x".repeat(1024*1024)}]},
        {"type":"commandExecution","id":"later","command":"test","status":"inProgress"}
    ])), vec![]).await;
    let value = settled(&fixture).await;
    assert_eq!(value[0]["result"]["truncated"], true);
    assert_eq!(value[1]["arguments"]["command"], "test");
    assert_eq!(value[2]["text"], "after");
    fixture.shutdown().await;
}

#[tokio::test]
async fn recovered_result_is_not_overwritten_by_a_covered_completion() {
    let current = json!({"type":"dynamicToolCall","id":"done","tool":"custom","arguments":{},"status":"completed","success":true,"contentItems":[{"type":"inputText","text":"authoritative"}]});
    let mut stale = current.clone();
    stale["contentItems"] = json!([]);
    let fixture = Fixture::new(turn(json!([current])), vec![item("item/completed", stale)]).await;
    let value = settled(&fixture).await;
    assert_eq!(
        value[0]["result"]["content"],
        json!([{"type":"inputText","text":"authoritative"}])
    );
    fixture.shutdown().await;
}

#[tokio::test]
async fn native_snapshot_covers_queued_command_output_and_retains_terminal_results() {
    let command = json!({"type":"commandExecution","id":"cmd","command":"test","status":"inProgress","aggregatedOutput":"ab"});
    let done = json!({"type":"dynamicToolCall","id":"done","tool":"custom","arguments":{},"status":"completed","success":false,"contentItems":[]});
    let fixture = Fixture::new(
        turn(json!([command, done.clone()])),
        vec![
            output("cmd", "a"),
            output("cmd", "b"),
            item("item/started", done),
        ],
    )
    .await;
    fixture.notify(output("cmd", "c")).await;
    let value = settled(&fixture).await;
    assert_eq!(value[0]["output"], json!({"text":"abc","truncated":true}));
    assert_eq!(
        value[1]["status"],
        json!({"phase":"ended","outcome":"failed"})
    );
    assert_eq!(value[1]["result"]["content"], json!([]));
    fixture.shutdown().await;
}
