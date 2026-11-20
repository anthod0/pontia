use super::*;
use pontia_agent_control::{
    AgentBindingService, AgentProfileService, standalone_session_instructions,
};

fn profiles(h: &Harness) -> AgentProfileService {
    AgentProfileService::new(h.app.db()).with_clients(h.app.clients())
}

async fn profile(h: &Harness, system: Option<&str>) {
    profiles(h)
        .create_profile(
            serde_json::from_value(json!({
                "profile_id":"reviewer", "version":"1", "name":"Reviewer",
                "agent_kind":"executor", "supported_client_types":["acp"],
                "system_prompt_template":system
            }))
            .unwrap(),
        )
        .await
        .unwrap();
}

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
    }
}

#[tokio::test]
async fn explicit_instructions_are_appended_and_pinned_across_history_and_restart() {
    let mut h = Harness::new(&[("ACP_SYSTEM_PROMPT", "1")]).await;
    profile(&h, Some("profile-marker")).await;
    let session = create(
        &h,
        json!({
            "execution_profile_id":"reviewer",
            "additional_instructions":"caller-marker",
            "include_standalone_session_instructions": true,
        }),
    )
    .await
    .unwrap();
    assert!(injected(&h, &session).await);
    sqlx::query("UPDATE execution_profiles SET system_prompt_template='changed-profile' WHERE profile_id='reviewer'")
        .execute(&h.app.db()).await.unwrap();
    h.history(&session).await.unwrap();
    h.restart().await;
    h.wait_ready(&session, 2).await;
    let expected = format!(
        "{}\n\nprofile-marker\n\ncaller-marker",
        standalone_session_instructions()
    );
    assert_prompts(&h, Some(&expected));
    assert_eq!(h.requests("session/new"), 1);
    assert!(h.requests("session/load") >= 2);
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
        json!({"execution_profile_id":"reviewer"}),
        json!({"additional_instructions":"caller-marker"}),
    ] {
        let h = Harness::new(&[]).await;
        profile(&h, Some("profile-marker")).await;
        assert!(matches!(
            create(&h, options).await,
            Err(Error::CapabilityUnavailable(_))
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
async fn profile_without_instructions_does_not_require_injection() {
    let h = Harness::new(&[]).await;
    profile(&h, None).await;
    let session = create(&h, json!({"execution_profile_id":"reviewer"}))
        .await
        .unwrap();
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
            create(&h, json!({"additional_instructions":"caller-marker"}))
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
    let mut h = Harness::new(&[("ACP_SYSTEM_PROMPT", "1")]).await;
    let session = create(&h, json!({"include_standalone_session_instructions": true}))
        .await
        .unwrap();
    h.release("no-system-prompt");
    h.restart().await;
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
async fn invalid_profile_templates_are_rejected() {
    let h = Harness::new(&[]).await;
    for templates in [
        json!({"system_prompt_template":" "}),
        json!({"system_prompt_template":"Hello {{name}}"}),
        json!({"turn_prompt_template":"per-turn"}),
    ] {
        let mut request = json!({"profile_id":"invalid", "version":"1", "name":"Invalid", "agent_kind":"executor", "supported_client_types":["acp"]});
        request
            .as_object_mut()
            .unwrap()
            .extend(templates.as_object().unwrap().clone());
        assert!(matches!(
            profiles(&h)
                .create_profile(serde_json::from_value(request).unwrap())
                .await,
            Err(Error::Domain(_))
        ));
    }
    h.stop().await;
}
