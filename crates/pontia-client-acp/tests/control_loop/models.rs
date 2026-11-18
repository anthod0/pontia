use super::*;

#[tokio::test]
async fn model_updates_preserve_the_active_turn_and_session_identity() {
    let h = Harness::new(&[
        ("ACP_MODELS", "flat"),
        ("ACP_LIVE_SCRIPT", "1"),
        ("ACP_NO_UPDATES", "1"),
    ])
    .await;
    let updated = json!({"sessionUpdate":"config_option_update", "configOptions":[{
        "id":"llm-selector", "name":"Model", "category":"model", "type":"select",
        "currentValue":"new-model", "options":[{"value":"new-model", "name":"New Model"}]
    }]});
    let steps = json!([
        [{"sessionId":"wrong-session", "update":updated}],
        [{"update":updated}],
        [{"update":{"sessionUpdate":"config_option_update", "configOptions":[]}}]
    ]);
    std::fs::write(h.root.path().join("live-script.json"), steps.to_string()).unwrap();
    let session = h.create("first").await.unwrap();
    let turn = h
        .app
        .turn_commands()
        .create_and_dispatch_turn(&session, "end_turn".into(), json!({}))
        .await
        .unwrap()
        .unwrap();
    h.release("update-1");
    for index in 0..3 {
        h.release(&format!("live-1-{index}"));
        tokio::time::timeout(Duration::from_secs(5), async {
            while !h.wire().iter().any(|message| {
                message["id"] == format!("live-barrier-1-{index}") && message.get("error").is_some()
            }) {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        let view = h
            .app
            .queries()
            .get_session(&session)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            view.model.as_deref(),
            Some(if index == 0 { "fast" } else { "new-model" })
        );
        assert_eq!(h.events(&session, "turn.started").await, 0);
    }
    h.release("release-1");
    h.wait_turn(&session, &turn.turn_id, "completed").await;
    h.stop().await;
}
