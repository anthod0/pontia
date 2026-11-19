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

async fn list(h: &Harness) -> Result<pontia_agent_control::agent_clients::AgentClientModels> {
    pontia_agent_control::agent_clients::AgentClientService::new(h.app.db(), h.app.clients())
        .list_models("acp", "first", h.root.path())
        .await
}

fn assert_probe_exited(h: &Harness) {
    let startup = h
        .wire()
        .into_iter()
        .find(|frame| frame.get("startup").is_some())
        .unwrap();
    let status = std::process::Command::new("kill")
        .args(["-0", &startup["startup"]["pid"].to_string()])
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(!status.success(), "model probe must be reaped");
}

#[tokio::test]
async fn model_discovery_uses_options_without_creating_a_session() {
    let h = Harness::new(&[
        ("ACP_OPTIONS", "1"),
        ("ACP_MODELS", "grouped"),
        ("ACP_NO_CLOSE", "1"),
        ("PONTIA_SESSION_ID", "not-a-probe-session"),
    ])
    .await;
    let models = list(&h).await.unwrap();
    assert_eq!(models.default_model.as_deref(), Some("fast"));
    assert_eq!(
        models
            .models
            .iter()
            .map(|model| model.id.as_str())
            .collect::<Vec<_>>(),
        ["fast", "deep"]
    );
    assert_eq!(models.models[0].description, "Fast model");
    assert_eq!(h.requests("session/options"), 1);
    assert_eq!(h.requests("session/new"), 0);
    assert_eq!(h.requests("session/prompt"), 0);
    let wire = h.wire();
    let startup = wire.iter().find_map(|frame| frame.get("startup")).unwrap();
    assert_eq!(startup["cwd"], h.root.path().to_str().unwrap());
    assert!(startup["session"].is_null());
    let sessions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions")
        .fetch_one(&h.app.db())
        .await
        .unwrap();
    assert_eq!(sessions, 0);
    assert_probe_exited(&h);
    h.stop().await;
}

#[tokio::test]
async fn model_discovery_closes_the_probe_session_when_options_are_not_advertised() {
    let h = Harness::new(&[("ACP_MODELS", "flat")]).await;
    let models = list(&h).await.unwrap();
    assert_eq!(models.default_model.as_deref(), Some("fast"));
    assert_eq!(models.models.len(), 2);
    assert_eq!(h.requests("session/options"), 0);
    assert_eq!(h.requests("session/new"), 1);
    assert_eq!(h.requests("session/close"), 1);
    assert_eq!(h.requests("session/prompt"), 0);
    assert_probe_exited(&h);
    h.stop().await;
}

#[tokio::test]
async fn failed_options_discovery_does_not_create_a_session() {
    let h = Harness::new(&[("ACP_OPTIONS", "error"), ("ACP_MODELS", "flat")]).await;
    assert!(list(&h).await.is_err());
    assert_eq!(h.requests("session/new"), 0);
    assert_probe_exited(&h);
    h.stop().await;
}

#[tokio::test]
async fn absent_model_options_return_an_empty_catalog_on_both_paths() {
    for flags in [vec![], vec![("ACP_OPTIONS", "1")]] {
        let h = Harness::new(&flags).await;
        let models = list(&h).await.unwrap();
        assert!(models.models.is_empty());
        assert!(models.default_model.is_none());
        assert_eq!(h.requests("session/new"), usize::from(flags.is_empty()));
        assert_probe_exited(&h);
        h.stop().await;
    }
}

#[tokio::test]
async fn probe_timeout_reaps_the_process_without_falling_back() {
    let h = Harness::new(&[("ACP_OPTIONS", "hold")]).await;
    let service =
        pontia_agent_control::agent_clients::AgentClientService::new(h.app.db(), h.app.clients());
    let directory = h.root.path().to_owned();
    let task = tokio::spawn(async move { service.list_models("acp", "first", &directory).await });
    h.wait_requests("session/options", 1).await;
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(31)).await;
    let result = task.await.unwrap();
    tokio::time::resume();
    assert!(matches!(result, Err(Error::CapabilityUnavailable(_))));
    assert_eq!(h.requests("session/new"), 0);
    assert_probe_exited(&h);
    h.stop().await;
}

async fn create_with_model(h: &Harness, model: &str) -> Result<String> {
    let request: CreateSessionRequest = serde_json::from_value(json!({
        "client_type":"acp", "workspace":h.root.path(),
        "metadata":{"acp_agent_id":"first"}, "model":model,
    }))
    .unwrap();
    let outcome = h.app.session_commands().create_session(request).await?;
    Ok(outcome.session_id().unwrap().to_owned())
}

#[tokio::test]
async fn selected_model_is_applied_before_the_first_prompt_and_preserved_on_resume() {
    let h = Harness::new(&[("ACP_MODELS", "new-only")]).await;
    let session = create_with_model(&h, "deep").await.unwrap();
    let view = h
        .app
        .queries()
        .get_session(&session)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(view.model.as_deref(), Some("deep"));
    let turn = h
        .app
        .turn_commands()
        .create_and_dispatch_turn(&session, "end_turn".into(), json!({}))
        .await
        .unwrap()
        .unwrap();
    h.release("update-1");
    h.release("release-1");
    h.wait_turn(&session, &turn.turn_id, "completed").await;
    assert!(h.wire().iter().any(|frame| frame["prompt_model"] == "deep"));
    h.app
        .session_commands()
        .terminate_session(&session)
        .await
        .unwrap();
    h.app
        .session_commands()
        .resume_session(&session, h.root.path())
        .await
        .unwrap();
    assert_eq!(h.requests("session/set_config_option"), 1);
    h.stop().await;
}

#[tokio::test]
async fn unavailable_rejected_or_ignored_models_fail_session_startup() {
    for (flags, selected) in [
        (vec![("ACP_MODELS", "flat")], "missing"),
        (
            vec![("ACP_MODELS", "flat"), ("ACP_REJECT_MODEL", "1")],
            "deep",
        ),
        (
            vec![("ACP_MODELS", "flat"), ("ACP_MODEL_OVERRIDE", "fast")],
            "deep",
        ),
        (vec![], "deep"),
    ] {
        let h = Harness::new(&flags).await;
        assert!(create_with_model(&h, selected).await.is_err());
        assert_eq!(h.requests("session/prompt"), 0);
        assert_probe_exited(&h);
        h.stop().await;
    }
}

#[tokio::test]
async fn model_notifications_after_selection_are_not_overwritten_by_the_response() {
    let h = Harness::new(&[("ACP_MODELS", "flat"), ("ACP_MODEL_AFTER_SET", "1")]).await;
    let session = create_with_model(&h, "deep").await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while !h
            .wire()
            .iter()
            .any(|frame| frame["id"] == "model-barrier" && frame.get("error").is_some())
        {
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
    assert_eq!(view.model.as_deref(), Some("fast"));
    h.stop().await;
}

#[tokio::test]
async fn failed_probe_close_is_reported_and_the_process_is_reaped() {
    let h = Harness::new(&[("ACP_MODELS", "flat"), ("ACP_CLOSE_FAIL", "1")]).await;
    assert!(list(&h).await.is_err());
    assert_probe_exited(&h);
    h.stop().await;
}
