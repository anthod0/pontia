use super::*;
use pontia_application::agent_clients::AgentClientService;

#[tokio::test]
async fn new_sessions_use_latest_overrides_while_existing_peers_remain_controllable() {
    let h = Harness::new(&[("ACP_UNIQUE_ID", "1")]).await;
    let management = AgentClientService::new(h.app.db(), h.app.clients());
    let first = h.create("first").await.unwrap();
    let binding = pontia_application::AgentBindingService::new(h.app.db())
        .binding_for_session(&first)
        .await
        .unwrap()
        .unwrap();
    management
        .save(
            "acp",
            "first",
            None,
            Some(json!({"env":{"ACP_LITERAL":"database"}})),
        )
        .await
        .unwrap();
    let second = h.create("first").await.unwrap();
    let untouched = h.create("second").await.unwrap();
    let startups: Vec<_> = h
        .wire()
        .into_iter()
        .filter_map(|value| value.get("startup").cloned())
        .collect();
    assert_eq!(startups.len(), 3);
    assert_eq!(startups[0]["literal"], "${NOT_EXPANDED} ~ *");
    assert_eq!(startups[1]["literal"], "database");
    assert_eq!(startups[2]["literal"], "${NOT_EXPANDED} ~ *");
    management
        .save("acp", "first", Some(false), None)
        .await
        .unwrap();
    assert!(matches!(
        h.create("first").await,
        Err(Error::CapabilityUnavailable(_))
    ));
    assert_eq!(h.requests("session/new"), 3);
    for session in [&first, &second, &untouched] {
        assert!(
            h.app
                .queries()
                .get_session(session)
                .await
                .unwrap()
                .unwrap()
                .capabilities
                .accept_task
        );
    }
    let turn = h
        .app
        .turn_commands()
        .create_and_dispatch_turn(&first, "end_turn".into(), json!({}))
        .await
        .unwrap()
        .unwrap();
    h.release("update-1");
    h.release("release-1");
    h.wait_turn(&first, &turn.turn_id, "completed").await;
    let after = pontia_application::AgentBindingService::new(h.app.db())
        .binding_for_session(&first)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(after.client_session_key, binding.client_session_key);
    assert_eq!(
        h.wire()
            .iter()
            .filter(|value| value.get("startup").is_some())
            .count(),
        3
    );
    management.delete("acp", "first").await.unwrap();
    assert!(matches!(
        h.create("first").await,
        Err(Error::CapabilityUnavailable(_))
    ));
    assert_eq!(h.requests("session/new"), 3);
    management
        .save("acp", "first", Some(true), None)
        .await
        .unwrap();
    h.create("first").await.unwrap();
    let startup = h
        .wire()
        .into_iter()
        .filter_map(|value| value.get("startup").cloned())
        .next_back()
        .unwrap();
    assert_eq!(startup["literal"], "${NOT_EXPANDED} ~ *");
    h.stop().await;
}

#[tokio::test]
async fn database_only_peer_can_launch_and_deletion_does_not_rebind_its_live_session() {
    let h = Harness::new(&[]).await;
    let management = AgentClientService::new(h.app.db(), h.app.clients());
    let config = json!({"command":"python3","args":[format!("{}/tests/fixtures/agent.py", env!("CARGO_MANIFEST_DIR"))],"env":{"ACP_FIXTURE_ROOT":h.root.path(),"ACP_LITERAL":"database-only"}});
    management
        .save("acp", "company", Some(true), Some(config.clone()))
        .await
        .unwrap();
    let options = h.app.clients().creation_options().await.unwrap();
    assert!(
        options
            .iter()
            .any(|option| option.metadata["acp_agent_id"] == "company")
    );
    let session = h.create("company").await.unwrap();
    let binding = pontia_application::AgentBindingService::new(h.app.db())
        .binding_for_session(&session)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        identity::decode(&binding.client_session_key).unwrap()[0],
        "company"
    );
    assert!(
        management
            .save("acp", "company", Some(false), Some(json!({"command":null})))
            .await
            .is_err()
    );
    let stored =
        pontia_storage_sqlite::repositories::agent_clients::SqliteAgentClientRepository::new(
            h.app.db(),
        )
        .get("acp", "company")
        .await
        .unwrap()
        .unwrap();
    assert!(stored.enabled);
    assert_eq!(
        serde_json::from_str::<Value>(&stored.config_overrides).unwrap(),
        config
    );
    management.delete("acp", "company").await.unwrap();
    assert!(h.create("company").await.is_err());
    assert!(
        h.app
            .queries()
            .get_session(&session)
            .await
            .unwrap()
            .unwrap()
            .capabilities
            .accept_task
    );
    assert_eq!(
        pontia_application::AgentBindingService::new(h.app.db())
            .binding_for_session(&session)
            .await
            .unwrap()
            .unwrap()
            .client_session_key,
        binding.client_session_key
    );
    h.stop().await;
}
