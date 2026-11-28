use super::*;
use pontia_agent_control::{AgentBindingService, standalone_session_instructions};

async fn create(h: &Harness, options: Value) -> Result<String> {
    let mut request = json!({
        "client_type":"acp", "workspace":h.root.path(), "metadata":{"acp_agent_id":"first"}
    });
    request
        .as_object_mut()
        .unwrap()
        .extend(options.as_object().unwrap().clone());
    let result = tokio::time::timeout(
        Duration::from_secs(5),
        h.app
            .session_commands()
            .create_session(serde_json::from_value(request).unwrap()),
    )
    .await
    .unwrap()?;
    Ok(result.session_id().unwrap().into())
}

async fn injected(h: &Harness, session: &str) -> bool {
    AgentBindingService::new(h.app.db())
        .binding_for_session(session)
        .await
        .unwrap()
        .unwrap()
        .metadata["system_prompt_injected"]
        .as_bool()
        .unwrap()
}

fn assert_prompts(h: &Harness, expected: Option<&str>) {
    let expected = expected
        .map(|text| {
            json!({
                "mode":"append", "content":[{"type":"text", "text":text}]
            })
        })
        .unwrap_or(Value::Null);
    let requests: Vec<_> = h
        .wire()
        .into_iter()
        .filter(|message| {
            matches!(
                message["method"].as_str(),
                Some("session/new" | "session/load")
            )
        })
        .collect();
    assert!(!requests.is_empty());
    for request in requests {
        assert_eq!(request["params"]["systemPrompt"], expected);
        assert!(request["params"]["_meta"].get("rules").is_none());
    }
}

#[tokio::test]
async fn explicit_instructions_are_appended_and_pinned_across_history_and_restart() {
    let mut h = Harness::new(&[("ACP_SYSTEM_PROMPT", "1")]).await;
    let session = create(
        &h,
        json!({
            "metadata":{"acp_agent_id":"second"},
            "system_instructions":"profile-marker\n\n{{name}}",
            "include_standalone_session_instructions": true,
        }),
    )
    .await
    .unwrap();
    assert!(injected(&h, &session).await);
    sqlx::query("DROP TABLE agent_profiles")
        .execute(&h.app.db())
        .await
        .unwrap();
    h.history(&session).await.unwrap();
    h.restart().await;
    h.wait_ready(&session, 2).await;
    let expected = format!(
        "{}\n\nprofile-marker\n\n{{{{name}}}}",
        standalone_session_instructions()
    );
    assert_prompts(&h, Some(&expected));
    assert_eq!(h.requests("session/new"), 1);
    assert!(h.requests("session/load") >= 2);
    h.stop().await;
}

#[tokio::test]
async fn native_instructions_are_sent_only_on_creation_and_remain_bound_on_recovery() {
    let h = Harness::new(&[]).await;
    let session = create(
        &h,
        json!({
            "metadata":{"acp_agent_id":"second"}, "system_instructions":"native-marker"
        }),
    )
    .await
    .unwrap();
    h.release("system-prompt");
    h.crash_agent(&session);
    h.wait_ready(&session, 2).await;
    assert!(injected(&h, &session).await);
    assert_eq!(h.requests("session/new"), 1);
    for request in h.wire() {
        let params = &request["params"];
        match request["method"].as_str() {
            Some("session/new") => {
                assert_eq!(params["_meta"]["rules"], "native-marker");
                assert!(params.get("systemPrompt").is_none());
            }
            Some("session/load") => {
                assert!(params["_meta"].get("rules").is_none());
                assert!(params.get("systemPrompt").is_none());
            }
            _ => {}
        }
    }
    h.stop().await;
}

#[tokio::test]
async fn antigravity_restores_its_nested_prompt_on_every_load() {
    let mut h = Harness::new(&[("ACP_ANTIGRAVITY", "1")]).await;
    let session = create(
        &h,
        json!({
            "metadata":{"acp_agent_id":"antigravity"}, "system_instructions":"profile-marker"
        }),
    )
    .await
    .unwrap();
    h.restart().await;
    h.wait_ready(&session, 2).await;
    h.release("system-prompt");
    h.crash_agent(&session);
    h.wait_ready(&session, 3).await;
    assert!(injected(&h, &session).await);
    assert_eq!(h.requests("session/new"), 1);
    h.app
        .session_commands()
        .terminate_session(&session)
        .await
        .unwrap();
    h.wait_session_state(&session, "exited").await;
    h.evict_history_cache(&session).await;
    let loads = h.requests("session/load");
    h.history(&session).await.unwrap();
    assert!(h.requests("session/load") > loads);
    for request in h.wire() {
        if matches!(
            request["method"].as_str(),
            Some("session/new" | "session/load")
        ) {
            let params = &request["params"];
            assert_eq!(
                params["_meta"],
                json!({"antigravity":{"systemPrompt":{"append":"profile-marker"}}})
            );
            assert!(params.get("systemPrompt").is_none());
        }
    }
    h.stop().await;
}

#[tokio::test]
async fn standalone_instructions_are_opt_in_and_injected_when_supported() {
    let h = Harness::new(&[("ACP_SYSTEM_PROMPT", "1")]).await;
    let session = h.create("first").await.unwrap();
    assert!(!injected(&h, &session).await);
    assert_prompts(&h, None);
    h.stop().await;

    let h = Harness::new(&[("ACP_SYSTEM_PROMPT", "1")]).await;
    let session = create(&h, json!({"include_standalone_session_instructions": true}))
        .await
        .unwrap();
    assert!(injected(&h, &session).await);
    assert_prompts(&h, Some(standalone_session_instructions()));
    h.stop().await;
}

#[tokio::test]
async fn standalone_instructions_can_be_skipped_and_are_not_added_after_agent_gains_support() {
    let mut h = Harness::new(&[]).await;
    let session = create(&h, json!({"include_standalone_session_instructions": true}))
        .await
        .unwrap();
    assert!(!injected(&h, &session).await);
    h.release("system-prompt");
    h.restart().await;
    h.wait_ready(&session, 2).await;
    assert!(!injected(&h, &session).await);
    assert_prompts(&h, None);
    assert_eq!(h.requests("session/new"), 1);
    h.stop().await;
}

#[tokio::test]
async fn unsupported_agent_rejects_explicit_instructions_before_native_creation() {
    for options in [
        json!({"system_instructions":"profile-marker"}),
        json!({"system_instructions":"caller-marker"}),
    ] {
        let h = Harness::new(&[]).await;
        assert!(matches!(
            create(&h, options).await,
            Err(Error::Unsupported(_))
        ));
        assert_eq!(h.requests("session/new"), 0);
        assert_eq!(h.requests("session/load"), 0);
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM agent_bindings")
            .fetch_one(&h.app.db())
            .await
            .unwrap();
        assert_eq!(count, 0);
        h.stop().await;
    }
}

#[tokio::test]
async fn session_without_instructions_does_not_require_injection() {
    let h = Harness::new(&[]).await;
    let session = create(&h, json!({})).await.unwrap();
    assert!(!injected(&h, &session).await);
    assert_prompts(&h, None);
    h.stop().await;
}

#[tokio::test]
async fn agent_rejection_does_not_silently_drop_instructions() {
    for stage in ["new", "load"] {
        let h = Harness::new(&[
            ("ACP_SYSTEM_PROMPT", "1"),
            ("ACP_REJECT_SYSTEM_PROMPT", stage),
        ])
        .await;
        assert!(
            create(&h, json!({"system_instructions":"caller-marker"}))
                .await
                .is_err()
        );
        assert_eq!(h.requests("session/new"), 1);
        let ready: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE event_type='session.ready'")
                .fetch_one(&h.app.db())
                .await
                .unwrap();
        assert_eq!(ready, 0);
        h.stop().await;
    }
}

#[tokio::test]
async fn recovery_fails_if_previously_injected_instructions_cannot_be_applied() {
    let h = Harness::new(&[("ACP_SYSTEM_PROMPT", "1")]).await;
    let session = create(&h, json!({"include_standalone_session_instructions": true}))
        .await
        .unwrap();
    h.release("no-system-prompt");
    h.crash_agent(&session);
    tokio::time::timeout(Duration::from_secs(5), async {
        while h.events(&session, "runtime.exited").await < 2 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(h.events(&session, "session.ready").await, 1);
    assert_eq!(h.requests("session/new"), 1);
    assert_eq!(h.requests("session/load"), 1);
    assert!(injected(&h, &session).await);
    h.stop().await;
}

#[tokio::test]
async fn invalid_session_instructions_are_rejected() {
    let h = Harness::new(&[]).await;
    for system in ["", " "] {
        assert!(matches!(
            create(&h, json!({"system_instructions":system})).await,
            Err(Error::Domain(_))
        ));
    }
    assert_eq!(h.requests("session/new"), 0);
    h.stop().await;
}

#[tokio::test]
async fn antigravity_prepares_its_workspace_agent_again_on_process_recovery() {
    let h = Harness::new(&[("ACP_ANTIGRAVITY", "1")]).await;
    let cli = h.root.path().join("clients/antigravity/antigravity-cli");
    std::fs::create_dir_all(&cli).unwrap();
    let token = cli.join("antigravity-oauth-token");
    std::fs::write(&token, "independent").unwrap();
    let cwd = h.root.path().join("bot/readonly");
    std::fs::create_dir_all(&cwd).unwrap();
    let session = create(
        &h,
        json!({
            "metadata":{"acp_agent_id":"antigravity"}, "mode":"restricted", "workspace":cwd
        }),
    )
    .await
    .unwrap();
    let agent = cwd.join(".agents/agents/pontia-restricted.md");
    let original = std::fs::read_to_string(&agent).unwrap();
    assert!(original.contains("tools: [read_file, run_command]"));
    std::fs::write(&agent, "old definition").unwrap();
    h.crash_agent(&session);
    h.wait_ready(&session, 2).await;
    assert_eq!(std::fs::read_to_string(agent).unwrap(), original);
    assert!(!token.is_symlink());
    assert_eq!(std::fs::read_to_string(token).unwrap(), "independent");
    let wire = h.wire();
    let launches: Vec<_> = wire.iter().filter_map(|item| item.get("startup")).collect();
    assert_eq!(launches.len(), 2);
    for launch in launches {
        assert_eq!(launch["cwd"], json!(cwd));
        assert_eq!(
            launch["argv"].as_array().unwrap()[1..],
            [
                json!("--gemini-dir"),
                json!(h.root.path().join("clients/antigravity")),
                json!("--agent"),
                json!("pontia-restricted"),
                json!("--include-default-customizations=false"),
                json!("--dangerously-skip-permissions")
            ]
        );
    }
    assert_eq!(h.requests("session/new"), 1);
    h.stop().await;
}

/// Creates a yolo Session in an ordinary workspace, then crashes, terminates,
/// reads exited history, and resumes it.
async fn exercise_yolo_session(h: &Harness, agent: &str) {
    let session = create(h, json!({"metadata":{"acp_agent_id":agent}, "mode":"yolo"}))
        .await
        .unwrap();
    h.crash_agent(&session);
    h.wait_ready(&session, 2).await;
    h.app
        .session_commands()
        .terminate_session(&session)
        .await
        .unwrap();
    h.evict_history_cache(&session).await;
    h.history(&session).await.unwrap();
    h.app
        .session_commands()
        .resume_session(&session, h.root.path())
        .await
        .unwrap();
    h.wait_ready(&session, 3).await;
}

#[tokio::test]
async fn antigravity_yolo_skips_permissions_with_the_user_configuration() {
    let h = Harness::new(&[("ACP_ANTIGRAVITY", "1")]).await;
    exercise_yolo_session(&h, "antigravity").await;
    let wire = h.wire();
    let launches: Vec<_> = wire.iter().filter_map(|item| item.get("startup")).collect();
    assert_eq!(launches.len(), 4, "{launches:?}");
    for launch in launches {
        assert_eq!(
            launch["argv"].as_array().unwrap()[1..],
            [json!("--dangerously-skip-permissions")]
        );
    }
    assert!(!h.root.path().join(".agents").exists());
    assert!(!h.root.path().join("clients/antigravity").exists());
    h.stop().await;
}

#[tokio::test]
async fn kiro_yolo_applies_the_allow_all_preset_to_every_native_session_request() {
    let h = Harness::new(&[("ACP_KIRO", "1")]).await;
    exercise_yolo_session(&h, "kiro").await;
    assert_eq!(h.requests("session/new"), 1);
    assert!(h.requests("session/load") >= 3);
    for frame in h.wire() {
        if matches!(
            frame["method"].as_str(),
            Some("session/new" | "session/load")
        ) {
            assert_eq!(
                frame["params"]["_meta"]["kiro"],
                json!({"policyPreset":["allow-all"]})
            );
        }
    }
    assert!(!h.root.path().join(".kiro").exists());
    h.stop().await;
}

#[tokio::test]
async fn ordinary_kiro_workspaces_keep_the_native_agent_and_their_configuration() {
    let h = Harness::new(&[("ACP_KIRO", "1")]).await;
    h.create("kiro").await.unwrap();
    for request in h.wire() {
        if matches!(
            request["method"].as_str(),
            Some("session/new" | "session/load")
        ) {
            assert!(request["params"]["_meta"].get("kiro").is_none());
        }
    }
    assert!(!h.root.path().join(".kiro").exists());
    h.stop().await;
}

#[tokio::test]
async fn kiro_selects_its_workspace_agent_on_creation_and_recovery() {
    let h = Harness::new(&[("ACP_KIRO", "1")]).await;
    let cwd = h.root.path().join("bot/readonly");
    std::fs::create_dir_all(&cwd).unwrap();
    let session = create(
        &h,
        json!({
            "metadata":{"acp_agent_id":"kiro"}, "mode":"restricted", "workspace":cwd
        }),
    )
    .await
    .unwrap();
    let agent = cwd.join(".kiro/agents/pontia-restricted.json");
    std::fs::write(&agent, "old agent").unwrap();
    let settings = cwd.join(".kiro/settings/cli.json");
    std::fs::write(&settings, "{}").unwrap();
    h.crash_agent(&session);
    h.wait_ready(&session, 2).await;
    assert_eq!(
        std::fs::read_to_string(settings).unwrap(),
        r#"{"chat.disableInheritingDefaultResources":true}"#
    );
    let definition: Value = serde_json::from_slice(&std::fs::read(agent).unwrap()).unwrap();
    assert_eq!(definition["tools"], json!(["read_file", "execute_bash"]));
    assert_eq!(h.requests("session/new"), 1);
    assert!(h.requests("session/load") >= 2);
    for frame in h.wire() {
        if matches!(
            frame["method"].as_str(),
            Some("session/new" | "session/load")
        ) {
            assert_eq!(
                frame["params"]["_meta"]["kiro"]["modeId"],
                "pontia-restricted"
            );
        }
    }
    h.stop().await;
}

#[tokio::test]
async fn kiro_prompts_include_bound_instructions_once_per_context_and_replay_user_text() {
    let mut h = Harness::new(&[("ACP_KIRO", "1"), ("ACP_STOP_REASON", "end_turn")]).await;
    let session = create(
        &h,
        json!({
            "metadata":{"acp_agent_id":"kiro"},
            "include_standalone_session_instructions":true,
            "system_instructions":"profile-marker"
        }),
    )
    .await
    .unwrap();
    assert!(injected(&h, &session).await);
    let inputs = ["first β", "after resume", "after compaction", "next"];
    for (i, input) in inputs.iter().enumerate() {
        if i == 1 {
            h.restart().await;
            h.wait_ready(&session, 2).await;
        }
        if i == 2 {
            use std::io::Write;
            let mut native_history = std::fs::OpenOptions::new()
                .append(true)
                .open(h.root.path().join("history.jsonl"))
                .unwrap();
            writeln!(
                native_history,
                "{}",
                json!({"sessionUpdate":"session_info_update",
                "_meta":{"kiro":{"kind":"summarization_separator","replay":true}}})
            )
            .unwrap();
            h.history(&session).await.unwrap();
        }
        let turn = h
            .app
            .turn_commands()
            .start_turn(&session, (*input).into(), json!({}))
            .await
            .unwrap()
            .unwrap();
        h.release(&format!("update-{}", i + 1));
        h.release(&format!("release-{}", i + 1));
        h.wait_turn(&session, &turn.turn_id, "completed").await;
    }
    let wire = h.wire();
    let sent: Vec<_> = wire
        .iter()
        .filter(|f| f["method"] == "session/prompt")
        .map(|f| f["params"]["prompt"][0]["text"].as_str().unwrap())
        .collect();
    assert_eq!(sent.len(), inputs.len());
    for i in [0, 2] {
        assert!(sent[i].contains(standalone_session_instructions()));
        assert!(sent[i].contains("profile-marker"));
        assert!(sent[i].ends_with(inputs[i]));
    }
    for i in [1, 3] {
        assert_eq!(sent[i], inputs[i]);
    }
    let page = h.history(&session).await.unwrap();
    let users: Vec<_> = page
        .items
        .iter()
        .filter(|i| i.item.role == "user")
        .map(|i| i.item.content_preview.as_str())
        .collect();
    assert_eq!(users, inputs);
    h.stop().await;
}

#[tokio::test]
async fn kiro_prefers_the_advertised_standard_system_prompt() {
    let h = Harness::new(&[("ACP_KIRO", "1"), ("ACP_SYSTEM_PROMPT", "1")]).await;
    let session = create(
        &h,
        json!({"metadata":{"acp_agent_id":"kiro"},"system_instructions":"profile-marker"}),
    )
    .await
    .unwrap();
    assert_prompts(&h, Some("profile-marker"));
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
    assert_eq!(
        h.wire()
            .into_iter()
            .find(|f| f["method"] == "session/prompt")
            .unwrap()["params"]["prompt"][0]["text"],
        "end_turn"
    );
    h.stop().await;
}
