use super::*;
use pontia_agent_control::sessions::SetSessionModelRequest;

async fn set_model(h: &Harness, session: &str, model: &str) -> Result<()> {
    let models = h
        .app
        .session_commands()
        .list_session_models(session)
        .await?;
    h.app
        .session_commands()
        .set_session_model(
            session,
            SetSessionModelRequest {
                model: model.into(),
                runtime_id: models.runtime_id,
            },
        )
        .await
}

#[tokio::test]
async fn lists_switches_and_restores_agent_models() {
    for layout in ["flat", "grouped", "new-only"] {
        let mut h = Harness::new(&[("ACP_MODELS", layout)]).await;
        let session = h.create("first").await.unwrap();
        let models = h
            .app
            .session_commands()
            .list_session_models(&session)
            .await
            .unwrap();
        assert_eq!(models.current_model.as_deref(), Some("fast"));
        assert_eq!(
            models
                .models
                .iter()
                .map(|model| model.id.as_str())
                .collect::<Vec<_>>(),
            ["fast", "deep"]
        );
        assert_eq!(models.models[0].description, "Fast model");
        set_model(&h, &session, "deep").await.unwrap();
        let models = h
            .app
            .session_commands()
            .list_session_models(&session)
            .await
            .unwrap();
        assert_eq!(models.current_model.as_deref(), Some("deep"));
        assert_eq!(
            std::fs::read_to_string(h.root.path().join("model.txt")).unwrap(),
            "deep"
        );
        if layout != "new-only" {
            h.restart().await;
            h.wait_ready(&session, 2).await;
            let models = h
                .app
                .session_commands()
                .list_session_models(&session)
                .await
                .unwrap();
            assert_eq!(models.current_model.as_deref(), Some("deep"));
            set_model(&h, &session, "fast").await.unwrap();
        }
        h.stop().await;
    }
}

#[tokio::test]
async fn agent_without_model_options_does_not_enable_model_control() {
    let h = Harness::new(&[]).await;
    let session = h.create("first").await.unwrap();
    let view = h
        .app
        .queries()
        .get_session(&session)
        .await
        .unwrap()
        .unwrap();
    assert!(!view.capabilities.list_models);
    assert!(!view.capabilities.set_model);
    assert!(matches!(
        h.app.session_commands().list_session_models(&session).await,
        Err(Error::CapabilityUnavailable(_))
    ));
    h.stop().await;
}

#[tokio::test]
async fn rejected_or_unlisted_model_preserves_current_model() {
    let h = Harness::new(&[("ACP_MODELS", "flat"), ("ACP_REJECT_MODEL", "1")]).await;
    let session = h.create("first").await.unwrap();
    assert!(matches!(
        set_model(&h, &session, "missing").await,
        Err(Error::Domain(_))
    ));
    assert_eq!(h.requests("session/set_config_option"), 0);
    assert!(matches!(
        set_model(&h, &session, "deep").await,
        Err(Error::Domain(_))
    ));
    let models = h
        .app
        .session_commands()
        .list_session_models(&session)
        .await
        .unwrap();
    assert_eq!(models.current_model.as_deref(), Some("fast"));
    h.stop().await;
}

#[tokio::test]
async fn agent_response_is_authoritative_for_selected_model() {
    let h = Harness::new(&[("ACP_MODELS", "flat"), ("ACP_MODEL_OVERRIDE", "fast")]).await;
    let session = h.create("first").await.unwrap();
    set_model(&h, &session, "deep").await.unwrap();
    let models = h
        .app
        .session_commands()
        .list_session_models(&session)
        .await
        .unwrap();
    assert_eq!(models.current_model.as_deref(), Some("fast"));
    h.stop().await;
}

#[tokio::test]
async fn idle_model_notification_after_response_remains_authoritative() {
    let h = Harness::new(&[("ACP_MODELS", "flat"), ("ACP_MODEL_AFTER_SET", "1")]).await;
    let session = h.create("first").await.unwrap();
    set_model(&h, &session, "deep").await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while !h
            .wire()
            .iter()
            .any(|message| message["id"] == "model-barrier" && message.get("error").is_some())
        {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    let models = h
        .app
        .session_commands()
        .list_session_models(&session)
        .await
        .unwrap();
    assert_eq!(models.current_model.as_deref(), Some("fast"));
    assert_eq!(h.events(&session, "turn.started").await, 0);
    h.stop().await;
}

#[tokio::test]
async fn model_updates_replace_choices_without_starting_a_turn_or_crossing_sessions() {
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
        if index < 2 {
            let models = h
                .app
                .session_commands()
                .list_session_models(&session)
                .await
                .unwrap();
            assert_eq!(
                models.current_model.as_deref(),
                Some(if index == 0 { "fast" } else { "new-model" })
            );
            if index == 1 {
                assert_eq!(
                    models
                        .models
                        .iter()
                        .map(|model| model.id.as_str())
                        .collect::<Vec<_>>(),
                    ["new-model"]
                );
            }
        } else {
            let view = h
                .app
                .queries()
                .get_session(&session)
                .await
                .unwrap()
                .unwrap();
            assert!(!view.capabilities.list_models);
            assert!(!view.capabilities.set_model);
        }
        assert_eq!(h.events(&session, "turn.started").await, 0);
    }
    h.release("release-1");
    h.wait_turn(&session, &turn.turn_id, "completed").await;
    h.stop().await;
}

#[tokio::test]
async fn model_switch_during_generation_preserves_active_turn() {
    let h = Harness::new(&[("ACP_MODELS", "flat")]).await;
    let session = h.create("first").await.unwrap();
    let turn = h
        .app
        .turn_commands()
        .create_and_dispatch_turn(&session, "end_turn".into(), json!({}))
        .await
        .unwrap()
        .unwrap();
    h.release("update-1");
    h.wait_turn(&session, &turn.turn_id, "running").await;
    set_model(&h, &session, "deep").await.unwrap();
    assert_eq!(
        h.app
            .session_commands()
            .list_session_models(&session)
            .await
            .unwrap()
            .current_model
            .as_deref(),
        Some("deep")
    );
    assert_eq!(
        h.app
            .queries()
            .get_turn(&session, &turn.turn_id)
            .await
            .unwrap()
            .unwrap()
            .state,
        "running"
    );
    h.release("release-1");
    h.wait_turn(&session, &turn.turn_id, "completed").await;
    h.stop().await;
}
