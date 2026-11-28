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
    }, {"id":"effort", "name":"Effort", "category":"thought_level", "type":"select",
        "currentValue":"low", "options":[{"value":"low", "name":"Low"}, {"value":"high","name":"High"}]}]});
    let mut effort_only = updated.clone();
    effort_only["configOptions"][1]["currentValue"] = json!("high");
    let steps = json!([
        [{"sessionId":"wrong-session", "update":updated}],
        [{"update":updated}],
        [{"update":effort_only}],
        [{"update":{"sessionUpdate":"config_option_update", "configOptions":[]}}]
    ]);
    std::fs::write(h.root.path().join("live-script.json"), steps.to_string()).unwrap();
    let session = h.create("first").await.unwrap();
    let turn = h
        .app
        .turn_commands()
        .start_turn(&session, "end_turn".into(), json!({}))
        .await
        .unwrap()
        .unwrap();
    h.release("update-1");
    for (index, effort) in [None, Some("low"), Some("high"), None]
        .into_iter()
        .enumerate()
    {
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
        assert_eq!(view.reasoning_effort.as_deref(), effort);
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
async fn delayed_model_options_support_discovery_and_startup_selection() {
    let h = Harness::new(&[("ACP_MODELS", "deferred"), ("ACP_DELETE", "1")]).await;
    assert_eq!(list(&h).await.unwrap().models.len(), 2);
    assert_eq!(h.requests("session/delete"), 1);
    let session = create_with_model(&h, "deep").await.unwrap();
    let view = h
        .app
        .queries()
        .get_session(&session)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(view.model.as_deref(), Some("deep"));
    h.stop().await;
}

#[tokio::test]
async fn unknown_agent_exposes_effort_only_for_the_current_model() {
    let h = Harness::new(&[("ACP_MODELS", "flat"), ("ACP_EFFORT", "1")]).await;
    let models = list(&h).await.unwrap().models;
    assert_eq!(
        models[0].default_reasoning_effort.as_deref(),
        Some("medium")
    );
    assert_eq!(
        models[0]
            .reasoning_efforts
            .iter()
            .map(|level| level.id.as_str())
            .collect::<Vec<_>>(),
        ["medium", "high", "low"]
    );
    assert_eq!(models[0].reasoning_efforts[1].name, "High");
    assert_eq!(models[0].reasoning_efforts[1].description, "More thinking");
    assert!(models[1].reasoning_efforts.is_empty());
    assert!(models[1].default_reasoning_effort.is_none());
    assert_eq!(h.requests("session/set_config_option"), 0);
    h.stop().await;
}

#[tokio::test]
async fn grok_catalog_reads_each_models_metadata_without_switching() {
    for options in [false, true] {
        let mut flags = vec![
            ("ACP_MODELS", "flat"),
            ("ACP_EFFORT", "1"),
            ("ACP_GROK_MODELS", "1"),
        ];
        if options {
            flags.push(("ACP_OPTIONS", "1"));
        }
        let h = Harness::new(&flags).await;
        let models = list(&h).await.unwrap();
        assert_eq!(models.default_model.as_deref(), Some("fast"));
        assert!(models.models[0].reasoning_efforts.is_empty());
        assert_eq!(models.models[0].default_reasoning_effort, None);
        let deep = &models.models[1];
        assert_eq!(deep.default_reasoning_effort.as_deref(), Some("max"));
        assert_eq!(deep.reasoning_efforts.len(), 2);
        assert_eq!(deep.reasoning_efforts[1].name, "Maximum");
        assert_eq!(deep.reasoning_efforts[1].description, "Thinks longer");
        assert_eq!(h.requests("session/set_config_option"), 0);
        h.stop().await;
    }
}

#[tokio::test]
async fn kiro_and_antigravity_collect_model_specific_efforts_and_restore_original() {
    for (key, mut flags) in [
        (
            "kiro",
            vec![
                ("ACP_KIRO", "1"),
                ("ACP_MODELS", "deferred"),
                ("ACP_DELETE", "1"),
            ],
        ),
        (
            "antigravity",
            vec![
                ("ACP_MODELS", "flat"),
                ("ACP_ANTIGRAVITY", "1"),
                ("ACP_PROBE_FAILURES", "1"),
            ],
        ),
    ] {
        flags.extend([("ACP_EFFORT", "1"), ("ACP_MODEL_EFFORT", "1")]);
        let h = Harness::new(&flags).await;
        let models = pontia_agent_control::agent_clients::AgentClientService::new(
            h.app.db(),
            h.app.clients(),
        )
        .list_models("acp", key, h.root.path())
        .await
        .unwrap();
        assert_eq!(models.default_model.as_deref(), Some("fast"));
        for model in &models.models {
            let levels: Vec<_> = model
                .reasoning_efforts
                .iter()
                .map(|level| level.id.as_str())
                .collect();
            let (expected, default): (&[&str], _) = match model.id.as_str() {
                "fast" => (&["medium", "high", "low"], Some("medium")),
                "deep" => (&["max"], Some("max")),
                _ => (&[], None),
            };
            assert_eq!(levels, expected);
            assert_eq!(model.default_reasoning_effort.as_deref(), default);
        }
        let selected: Vec<_> = h
            .wire()
            .into_iter()
            .filter(|frame| frame["method"] == "session/set_config_option")
            .map(|frame| frame["params"]["value"].as_str().unwrap().to_owned())
            .collect();
        if flags.contains(&("ACP_PROBE_FAILURES", "1")) {
            assert_eq!(selected, ["stalled", "rejected", "ignored", "deep", "fast"]);
        } else {
            assert_eq!(selected, ["deep", "fast"]);
            assert_eq!(h.requests("session/delete"), 1);
        }
        assert_eq!(h.requests("session/close"), 1);
        h.stop().await;
    }
}

#[tokio::test]
async fn restore_applies_the_observed_effort_after_model_selection() {
    let h = Harness::new(&[
        ("ACP_MODELS", "flat"),
        ("ACP_EFFORT", "1"),
        ("ACP_EFFECTIVE_EFFORT", "low"),
    ])
    .await;
    let request = serde_json::from_value(json!({
        "client_type":"acp", "workspace":h.root.path(), "metadata":{"acp_agent_id":"first"},
        "model":"deep", "reasoning_effort":"high"
    }))
    .unwrap();
    let created = h
        .app
        .session_commands()
        .create_session(request)
        .await
        .unwrap();
    let session = created.session_id().unwrap();
    let view = h.app.queries().get_session(session).await.unwrap().unwrap();
    assert_eq!(view.reasoning_effort.as_deref(), Some("low"));
    h.app
        .session_commands()
        .terminate_session(session)
        .await
        .unwrap();
    h.app
        .session_commands()
        .resume_session(session, h.root.path())
        .await
        .unwrap();
    let settings: Vec<_> = h
        .wire()
        .into_iter()
        .filter(|frame| frame["method"] == "session/set_config_option")
        .map(|frame| json!([frame["params"]["configId"], frame["params"]["value"]]))
        .collect();
    assert_eq!(
        settings,
        [
            json!(["llm-selector", "deep"]),
            json!(["deep-effort", "high"]),
            json!(["deep-effort", "low"])
        ]
    );
    assert_eq!(
        h.events(session, "session.reasoning_effort_updated").await,
        2
    );
    h.stop().await;
}

#[tokio::test]
async fn connected_sessions_expose_and_apply_confirmed_reasoning_effort() {
    let h = Harness::new(&[
        ("ACP_MODELS", "flat"),
        ("ACP_EFFORT", "1"),
        ("ACP_EFFECTIVE_EFFORT", "low"),
    ])
    .await;
    let session = h.create("first").await.unwrap();
    let initial = h
        .app
        .queries()
        .get_session(&session)
        .await
        .unwrap()
        .unwrap();
    assert!(initial.capabilities.reasoning_effort_control);
    assert_eq!(initial.reasoning_effort.as_deref(), Some("medium"));

    h.app
        .session_commands()
        .set_reasoning_effort(
            &session,
            pontia_agent_control::SetSessionReasoningEffortRequest {
                reasoning_effort: "high".into(),
            },
        )
        .await
        .unwrap();

    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let view = h
                .app
                .queries()
                .get_session(&session)
                .await
                .unwrap()
                .unwrap();
            if view.reasoning_effort.as_deref() == Some("low") {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    let request = h
        .wire()
        .into_iter()
        .find(|frame| frame["method"] == "session/set_config_option")
        .unwrap();
    assert_eq!(request["params"]["configId"], "fast-effort");
    assert_eq!(request["params"]["value"], "high");
    assert_eq!(
        h.events(&session, "session.reasoning_effort_updated").await,
        2
    );
    h.stop().await;
}

#[tokio::test]
async fn sessions_without_a_thought_level_selector_cannot_change_reasoning_effort() {
    let h = Harness::new(&[("ACP_MODELS", "flat")]).await;
    let session = h.create("first").await.unwrap();
    let view = h
        .app
        .queries()
        .get_session(&session)
        .await
        .unwrap()
        .unwrap();
    assert!(!view.capabilities.reasoning_effort_control);
    assert!(matches!(
        h.app
            .session_commands()
            .set_reasoning_effort(
                &session,
                pontia_agent_control::SetSessionReasoningEffortRequest {
                    reasoning_effort: "high".into(),
                },
            )
            .await,
        Err(Error::Unsupported(_))
    ));
    assert_eq!(h.requests("session/set_config_option"), 0);
    h.stop().await;
}

#[tokio::test]
async fn kiro_model_discovery_avoids_creating_a_session_it_cannot_delete() {
    let h = Harness::new(&[("ACP_MODELS", "deferred")]).await;
    assert!(matches!(
        list(&h).await,
        Err(Error::CapabilityUnavailable(_))
    ));
    assert_eq!(h.requests("session/new"), 0);
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
        .start_turn(&session, "end_turn".into(), json!({}))
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
async fn resumed_prompt_uses_the_saved_model_when_load_reports_a_different_default() {
    let h = Harness::new(&[("ACP_MODELS", "flat")]).await;
    let session = create_with_model(&h, "deep").await.unwrap();
    h.app
        .session_commands()
        .terminate_session(&session)
        .await
        .unwrap();
    std::fs::write(h.root.path().join("model.txt"), "fast").unwrap();

    h.app
        .session_commands()
        .resume_session(&session, h.root.path())
        .await
        .unwrap();
    let turn = h
        .app
        .turn_commands()
        .start_turn(&session, "end_turn".into(), json!({}))
        .await
        .unwrap()
        .unwrap();
    h.release("update-1");
    h.release("release-1");
    h.wait_turn(&session, &turn.turn_id, "completed").await;
    assert!(h.wire().iter().any(|frame| frame["prompt_model"] == "deep"));
    h.stop().await;
}

#[tokio::test]
async fn rejected_model_restore_keeps_the_previous_model_for_retry() {
    let h = Harness::new(&[("ACP_MODELS", "flat"), ("ACP_REJECT_MODEL", "1")]).await;
    let model = h.root.path().join("model.txt");
    std::fs::write(&model, "deep").unwrap();
    let session = h.create("first").await.unwrap();
    h.app
        .session_commands()
        .terminate_session(&session)
        .await
        .unwrap();
    std::fs::write(model, "fast").unwrap();

    let result = h
        .app
        .session_commands()
        .resume_session(&session, h.root.path())
        .await;
    assert!(result.is_err());
    let view = h
        .app
        .queries()
        .get_session(&session)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(view.model.as_deref(), Some("deep"));
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

async fn restricted_request(h: &Harness, model: &str) -> Result<String> {
    let request: CreateSessionRequest = serde_json::from_value(json!({
        "client_type":"acp", "workspace":h.root.path(), "metadata":{"acp_agent_id":"second"},
        "model":model, "system_instructions":"Bound rules",
        "mode":"restricted"
    }))
    .unwrap();
    let outcome = h.app.session_commands().create_session(request).await?;
    Ok(outcome.session_id().unwrap().to_owned())
}

#[tokio::test]
async fn restricted_session_merges_rules_and_retains_policy_on_resume() {
    let h = Harness::new(&[("ACP_MODELS", "flat")]).await;
    let session = restricted_request(&h, "deep").await.unwrap();
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
    let wire = h.wire();
    let creation = wire.iter().find(|v| v["method"] == "session/new").unwrap();
    let meta = &creation["params"]["_meta"];
    assert_eq!(meta["modelId"], "deep");
    assert!(meta["rules"].as_str().unwrap().contains("Bound rules"));
    assert_eq!(meta["yoloMode"], true);
    assert_eq!(meta["agentProfile"]["discoverSkills"], false);
    assert_eq!(
        meta["agentProfile"]["tools"],
        json!(["read_file", "run_terminal_cmd"])
    );
    assert_eq!(
        meta["agentProfile"]["disallowedTools"],
        json!(["search_tool", "use_tool"])
    );
    assert!(h.requests("session/load") >= 2);
    for load in wire.iter().filter(|v| v["method"] == "session/load") {
        let loaded = &load["params"]["_meta"];
        assert_eq!(loaded["agentProfile"], meta["agentProfile"]);
        assert_eq!(loaded["yoloMode"], true);
        assert!(loaded.get("rules").is_none());
    }
    h.stop().await;
}

#[tokio::test]
async fn externally_injected_models_are_allowed_in_restricted_sessions() {
    let h = Harness::new(&[("ACP_MODELS", "flat")]).await;
    std::fs::write(h.root.path().join("model.txt"), "third-party").unwrap();
    restricted_request(&h, "third-party").await.unwrap();
    assert_eq!(h.requests("session/new"), 1);
    h.stop().await;
}

#[tokio::test]
async fn catalog_models_without_agent_types_are_allowed_in_restricted_sessions() {
    let h = Harness::new(&[("ACP_MODELS", "flat")]).await;
    std::fs::write(h.root.path().join("model.txt"), "unclassified").unwrap();
    restricted_request(&h, "unclassified").await.unwrap();
    assert_eq!(h.requests("session/new"), 1);
    h.stop().await;
}

#[tokio::test]
async fn catalog_models_with_incompatible_agent_types_fail_before_native_session_creation() {
    let h = Harness::new(&[("ACP_MODELS", "flat")]).await;
    std::fs::write(h.root.path().join("model-kind"), "grok-build-orchestrator").unwrap();
    assert!(matches!(
        restricted_request(&h, "fast").await,
        Err(Error::CapabilityUnavailable(_))
    ));
    assert_eq!(h.requests("session/new"), 0);
    assert_eq!(h.requests("session/prompt"), 0);
    h.stop().await;
}

#[tokio::test]
async fn changed_profile_blocks_resume_before_prompt() {
    let h = Harness::new(&[("ACP_MODELS", "flat")]).await;
    let session = restricted_request(&h, "fast").await.unwrap();
    let wire = h.wire();
    let new = wire.iter().find(|v| v["method"] == "session/new").unwrap();
    assert_eq!(
        new["params"]["_meta"]["agentProfile"]["discoverSkills"],
        false
    );
    h.app
        .session_commands()
        .terminate_session(&session)
        .await
        .unwrap();
    h.release("wrong-profile");
    assert!(
        h.app
            .session_commands()
            .resume_session(&session, h.root.path())
            .await
            .is_err()
    );
    assert_eq!(h.requests("session/prompt"), 0);
    h.stop().await;
}

#[tokio::test]
async fn model_type_is_checked_again_before_switching_on_resume() {
    let h = Harness::new(&[("ACP_MODELS", "flat")]).await;
    let session = restricted_request(&h, "deep").await.unwrap();
    h.app
        .session_commands()
        .terminate_session(&session)
        .await
        .unwrap();
    let switches = h.requests("session/set_config_option");
    std::fs::write(h.root.path().join("model.txt"), "fast").unwrap();
    h.release("strict-deep");
    assert!(
        h.app
            .session_commands()
            .resume_session(&session, h.root.path())
            .await
            .is_err()
    );
    assert_eq!(h.requests("session/set_config_option"), switches);
    assert_eq!(h.requests("session/prompt"), 0);
    h.stop().await;
}

#[tokio::test]
async fn effective_model_must_be_verified_even_when_requested_model_is_allowed() {
    let h = Harness::new(&[("ACP_MODELS", "flat")]).await;
    std::fs::write(h.root.path().join("model.txt"), "deep").unwrap();
    h.release("strict-deep");
    assert!(restricted_request(&h, "fast").await.is_err());
    assert_eq!(h.requests("session/new"), 1);
    assert_eq!(h.requests("session/prompt"), 0);
    h.stop().await;
}
