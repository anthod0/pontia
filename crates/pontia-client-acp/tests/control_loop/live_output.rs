use super::*;
use pontia_application::LiveOutputStreamEvent;

fn text(text: &str) -> Value {
    json!([{"type":"content","content":{"type":"text","text":text}}])
}

fn notification(update: Value) -> Value {
    json!({"update": update})
}

async fn start(steps: Vec<Vec<Value>>) -> (Harness, String, String) {
    let h = Harness::new(&[("ACP_LIVE_SCRIPT", "1"), ("ACP_NO_UPDATES", "1")]).await;
    std::fs::write(
        h.root.path().join("live-script.json"),
        serde_json::to_vec(&steps).unwrap(),
    )
    .unwrap();
    let session = h.create("first").await.unwrap();
    let turn = h
        .app
        .turn_commands()
        .create_and_dispatch_turn(&session, "end_turn".into(), json!({}))
        .await
        .unwrap()
        .unwrap()
        .turn_id;
    h.release("update-1");
    (h, session, turn)
}

async fn step(h: &Harness, index: usize) {
    h.release(&format!("live-1-{index}"));
    tokio::time::timeout(Duration::from_secs(5), async {
        while !h.wire().iter().any(|frame| {
            frame["id"] == format!("live-barrier-1-{index}") && frame.get("error").is_some()
        }) {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("script notification barrier");
}

fn snapshot(h: &Harness, session: &str, turn: &str) -> Value {
    serde_json::to_value(h.app.live_output().snapshot(session, turn).unwrap()).unwrap()
}

fn tool<'a>(snapshot: &'a Value, id: &str) -> &'a Value {
    snapshot["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["call_id"] == id)
        .unwrap()
}

#[tokio::test]
async fn interleaved_tools_and_text_survive_partial_call_snapshots_and_reconnect() {
    let (h, session, turn) = start(vec![
        vec![notification(json!({"sessionUpdate":"tool_call","toolCallId":"a","title":"Execute","kind":"execute","status":"pending","rawInput":{"command":"first"}}))],
        vec![
            notification(json!({"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"before"}})),
            notification(json!({"sessionUpdate":"tool_call","toolCallId":"b","title":"Execute","rawInput":{"command":"second"}})),
            notification(json!({"sessionUpdate":"tool_call_update","toolCallId":"a","status":"in_progress","content":text("a")})),
        ],
        vec![
            notification(json!({"sessionUpdate":"tool_call_update","toolCallId":"b","status":"failed","content":text("failure"),"rawOutput":{"code":7}})),
            notification(json!({"sessionUpdate":"tool_call_update","toolCallId":"a","content":text("ab")})),
            notification(json!({"sessionUpdate":"tool_call_update","toolCallId":"a","title":"Execute updated","rawInput":{"command":"updated"}})),
        ],
        vec![
            notification(json!({"sessionUpdate":"tool_call_update","toolCallId":"a","status":"completed","rawOutput":{"stdout":"ab"}})),
            notification(json!({"sessionUpdate":"tool_call","toolCallId":"a","title":"Execute updated","status":"pending"})),
            notification(json!({"sessionUpdate":"tool_call_update","toolCallId":"a","status":"in_progress"})),
            notification(json!({"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":" after"}})),
        ],
    ]).await;
    let mut stream = h.app.live_output().subscribe_session(&session);
    step(&h, 0).await;
    let first = snapshot(&h, &session, &turn);
    assert_eq!(first["items"].as_array().unwrap().len(), 1);
    assert!(tool(&first, "a").get("status").is_none());
    assert!(tool(&first, "a").get("managed_tool_use").is_none());
    step(&h, 1).await;
    let running = snapshot(&h, &session, &turn);
    assert_eq!(tool(&running, "a")["status"], json!({"phase":"running"}));
    assert_eq!(tool(&running, "a")["output"]["text"], "a");
    assert!(tool(&running, "a").get("result").is_none());
    step(&h, 2).await;
    let partial = snapshot(&h, &session, &turn);
    assert_eq!(partial["items"].as_array().unwrap().len(), 3);
    assert_eq!(
        tool(&partial, "a")["arguments"],
        json!({"command":"updated"})
    );
    assert_eq!(tool(&partial, "a")["output"]["text"], "ab");
    assert_eq!(
        tool(&partial, "b")["arguments"],
        json!({"command":"second"})
    );
    assert_eq!(tool(&partial, "b")["result"]["error"], json!({"code":7}));
    assert_eq!(tool(&partial, "b")["status"]["outcome"], "failed");
    h.wait_turn(&session, &turn, "running").await;
    let mut reconnected = h.app.live_output().subscribe_session(&session);
    assert_eq!(
        serde_json::to_value(reconnected.initial_snapshot.take().unwrap()).unwrap(),
        partial
    );
    step(&h, 3).await;
    assert!(matches!(
        reconnected.recv().await.unwrap(),
        LiveOutputStreamEvent::Updates { .. }
    ));
    let finished = snapshot(&h, &session, &turn);
    assert_eq!(finished["items"].as_array().unwrap().len(), 3);
    assert_eq!(
        tool(&finished, "a")["status"],
        json!({"phase":"ended","outcome":"succeeded"})
    );
    assert_eq!(tool(&finished, "a")["output"]["text"], "ab");
    assert_eq!(
        tool(&finished, "a")["result"]["content"],
        json!({"stdout":"ab"})
    );
    assert_eq!(
        tool(&finished, "a")["arguments"],
        json!({"command":"updated"})
    );
    assert_eq!(finished["items"][1]["text"], "before after");
    // The first tool arrives as a snapshot; subsequent discoveries use exactly
    // one call update, while later call metadata changes use snapshots.
    let mut calls = Vec::new();
    while let Ok(Ok(event)) = tokio::time::timeout(Duration::from_millis(20), stream.recv()).await {
        if let LiveOutputStreamEvent::Updates { updates, .. } = event {
            for update in updates {
                if let pontia_application::LiveOutputUpdate::ToolCall { call_id, .. } = update {
                    calls.push(call_id);
                }
            }
        }
    }
    assert_eq!(calls, ["b"]);
    h.release("release-1");
    h.wait_turn(&session, &turn, "completed").await;
    assert!(h.app.live_output().snapshot(&session, &turn).is_none());
    h.stop().await;
}

#[tokio::test]
async fn unassociated_and_non_display_notifications_do_not_create_live_content() {
    let (h, session, turn) = start(vec![vec![
        notification(json!({"sessionUpdate":"tool_call_update","toolCallId":"unknown","status":"failed","content":text("orphan"),"rawOutput":"orphan"})),
        notification(json!({"sessionUpdate":"agent_thought_chunk","content":{"type":"text","text":"private"}})),
        notification(json!({"sessionUpdate":"plan","entries":[]})),
        json!({"sessionId":"wrong","update":{"sessionUpdate":"tool_call","toolCallId":"wrong","title":"Wrong"}}),
    ]]).await;
    step(&h, 0).await;
    assert!(h.app.live_output().snapshot(&session, &turn).is_none());
    h.stop().await;
}

#[tokio::test]
async fn update_only_discovery_preserves_omitted_content_and_clears_explicit_empty_content() {
    let (h, session, turn) = start(vec![
        vec![notification(json!({"sessionUpdate":"tool_call_update","toolCallId":"a","title":"Discovered","content":text("a")}))],
        vec![notification(json!({"sessionUpdate":"tool_call_update","toolCallId":"a","status":"in_progress"}))],
        vec![notification(json!({"sessionUpdate":"tool_call_update","toolCallId":"a","content":[]}))],
    ]).await;
    step(&h, 0).await;
    step(&h, 1).await;
    assert_eq!(
        tool(&snapshot(&h, &session, &turn), "a")["output"]["text"],
        "a"
    );
    step(&h, 2).await;
    assert_eq!(
        tool(&snapshot(&h, &session, &turn), "a")["output"]["text"],
        ""
    );
    h.stop().await;
}

#[tokio::test]
async fn results_distinguish_missing_null_and_actual_failure_content() {
    let (h, session, turn) = start(vec![vec![
        notification(json!({"sessionUpdate":"tool_call","toolCallId":"a","title":"No return","status":"completed"})),
        notification(json!({"sessionUpdate":"tool_call","toolCallId":"b","title":"Null return","status":"completed","rawOutput":null})),
        notification(json!({"sessionUpdate":"tool_call","toolCallId":"c","title":"Text failure","status":"failed","content":text("error details")})),
    ]]).await;
    step(&h, 0).await;
    let done = snapshot(&h, &session, &turn);
    assert!(tool(&done, "a").get("result").is_none());
    assert_eq!(tool(&done, "a")["status"]["outcome"], "succeeded");
    assert_eq!(
        tool(&done, "b")["result"],
        json!({"content":null,"truncated":false})
    );
    assert_eq!(tool(&done, "c")["result"]["error"], "error details");
    h.stop().await;
}

#[tokio::test]
async fn unsupported_tool_content_does_not_manufacture_text_but_replaces_prior_output() {
    let content = json!([{"type":"terminal","terminalId":"native-terminal"}]);
    let (h, session, turn) = start(vec![
        vec![notification(json!({"sessionUpdate":"tool_call","toolCallId":"a","title":"Terminal","content":content}))],
        vec![notification(json!({"sessionUpdate":"tool_call_update","toolCallId":"a","content":text("visible")}))],
        vec![notification(json!({"sessionUpdate":"tool_call_update","toolCallId":"a","status":"completed","content":content}))],
    ]).await;
    step(&h, 0).await;
    assert!(
        tool(&snapshot(&h, &session, &turn), "a")
            .get("output")
            .is_none()
    );
    step(&h, 1).await;
    assert_eq!(
        tool(&snapshot(&h, &session, &turn), "a")["output"]["text"],
        "visible"
    );
    step(&h, 2).await;
    let done = snapshot(&h, &session, &turn);
    assert_eq!(tool(&done, "a")["output"]["text"], "");
    assert!(tool(&done, "a").get("result").is_none());
    h.stop().await;
}

#[tokio::test]
async fn textual_resource_content_is_available_as_output_and_terminal_result() {
    let content = json!([{"type":"content","content":{"type":"resource","resource":{
        "uri":"test://tool-result", "mimeType":"text/plain", "text":"resource output"
    }}}]);
    let (h, session, turn) = start(vec![
        vec![notification(json!({"sessionUpdate":"tool_call","toolCallId":"a","title":"Resource","content":content}))],
        vec![notification(json!({"sessionUpdate":"tool_call_update","toolCallId":"a","status":"completed","content":content}))],
    ]).await;
    step(&h, 0).await;
    let running = snapshot(&h, &session, &turn);
    assert_eq!(tool(&running, "a")["output"]["text"], "resource output");
    assert!(tool(&running, "a").get("result").is_none());
    step(&h, 1).await;
    assert_eq!(
        tool(&snapshot(&h, &session, &turn), "a")["result"]["content"],
        "resource output"
    );
    h.stop().await;
}

#[tokio::test]
async fn later_content_replacements_preserve_native_returns_until_explicitly_replaced() {
    let (h, session, turn) = start(vec![
        vec![notification(json!({"sessionUpdate":"tool_call","toolCallId":"a","title":"Return","status":"completed","rawOutput":{"exitCode":0}}))],
        vec![notification(json!({"sessionUpdate":"tool_call_update","toolCallId":"a","status":"completed","content":text("display changed")}))],
        vec![notification(json!({"sessionUpdate":"tool_call_update","toolCallId":"a","rawOutput":null}))],
        vec![notification(json!({"sessionUpdate":"tool_call_update","toolCallId":"a","status":"completed","content":text("display changed again")}))],
    ]).await;
    step(&h, 0).await;
    step(&h, 1).await;
    let updated = snapshot(&h, &session, &turn);
    assert_eq!(
        tool(&updated, "a")["result"]["content"],
        json!({"exitCode":0})
    );
    assert_eq!(tool(&updated, "a")["output"]["text"], "display changed");
    step(&h, 2).await;
    step(&h, 3).await;
    let updated = snapshot(&h, &session, &turn);
    assert_eq!(
        tool(&updated, "a")["result"],
        json!({"content":null,"truncated":false})
    );
    assert_eq!(
        tool(&updated, "a")["output"]["text"],
        "display changed again"
    );
    h.stop().await;
}

#[tokio::test]
async fn shared_capacity_limits_reject_calls_and_bound_output_without_poisoning_following_updates()
{
    let large = "界".repeat(400_000);
    let (h, session, turn) = start(vec![
        vec![
            notification(json!({"sessionUpdate":"tool_call","toolCallId":"oversized","title":"Large input","rawInput":"x".repeat(65536)})),
            notification(json!({"sessionUpdate":"tool_call","toolCallId":"a","title":"Valid","rawInput":{}})),
            notification(json!({"sessionUpdate":"tool_call_update","toolCallId":"a","content":text(&large),"rawOutput":large})),
        ],
        vec![notification(json!({"sessionUpdate":"tool_call_update","toolCallId":"a","content":[],"rawOutput":{"small":true}}))],
        (0..256).map(|i| notification(json!({"sessionUpdate":"tool_call","toolCallId":format!("call-{i}"),"title":"Tool"}))).collect(),
        vec![notification(json!({"sessionUpdate":"tool_call_update","toolCallId":"a","status":"completed"}))],
    ]).await;
    step(&h, 0).await;
    let bounded = snapshot(&h, &session, &turn);
    assert_eq!(bounded["items"].as_array().unwrap().len(), 1);
    let a = tool(&bounded, "a");
    assert_eq!(a["output"]["truncated"], true);
    assert!(a["output"]["text"].as_str().unwrap().len() < 1024 * 1024);
    assert_eq!(a["result"]["truncated"], true);
    step(&h, 1).await;
    let replaced = snapshot(&h, &session, &turn);
    assert_eq!(
        tool(&replaced, "a")["output"],
        json!({"text":"","truncated":false})
    );
    assert_eq!(
        tool(&replaced, "a")["result"],
        json!({"content":{"small":true},"truncated":false})
    );
    step(&h, 2).await;
    step(&h, 3).await;
    let full = snapshot(&h, &session, &turn);
    assert_eq!(full["items"].as_array().unwrap().len(), 256);
    assert_eq!(tool(&full, "a")["status"]["outcome"], "succeeded");
    h.stop().await;
}

#[tokio::test]
async fn load_replay_never_creates_live_tools() {
    let h = Harness::new(&[]).await;
    std::fs::write(h.root.path().join("history.jsonl"), format!("{}\n{}\n",
        json!({"sessionUpdate":"tool_call","toolCallId":"history","title":"Old tool","status":"in_progress"}),
        json!({"sessionUpdate":"tool_call_update","toolCallId":"history","status":"completed","content":text("old output")}))).unwrap();
    let session = h.create("first").await.unwrap();
    assert!(
        h.app
            .live_output()
            .subscribe_session(&session)
            .initial_snapshot
            .is_none()
    );
    h.history(&session).await.unwrap();
    assert!(
        h.app
            .live_output()
            .subscribe_session(&session)
            .initial_snapshot
            .is_none()
    );
    h.stop().await;
}
