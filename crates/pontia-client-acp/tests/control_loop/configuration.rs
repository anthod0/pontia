use super::*;
use pontia_agent_control::agent_clients::AgentClientService;

#[tokio::test]
async fn disabling_and_deleting_a_record_only_affects_new_sessions() {
    let h = Harness::new(&[("ACP_UNIQUE_ID", "1")]).await;
    let management = AgentClientService::new(h.app.db(), h.app.clients());
    let first = h.create("first").await.unwrap();
    let binding = pontia_agent_control::AgentBindingService::new(h.app.db())
        .binding_for_session(&first)
        .await
        .unwrap()
        .unwrap();
    management
        .save("acp", "first", Some(false), None)
        .await
        .unwrap();
    assert!(matches!(
        h.create("first").await,
        Err(Error::CapabilityUnavailable(_))
    ));
    assert_eq!(h.requests("session/new"), 1);
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
    let after = pontia_agent_control::AgentBindingService::new(h.app.db())
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
        1
    );
    management.delete("acp", "first").await.unwrap();
    assert!(matches!(
        h.create("first").await,
        Err(Error::CapabilityUnavailable(_))
    ));
    assert_eq!(h.requests("session/new"), 1);
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
async fn removed_definition_keeps_live_session_control_and_cold_binding_details() {
    let mut h = Harness::new(&[]).await;
    let session = h.create("first").await.unwrap();
    let before = h
        .app
        .queries()
        .get_session(&session)
        .await
        .unwrap()
        .unwrap();
    h.config = AcpAgents::new([]);
    h.restart().await;
    h.wait_ready(&session, 2).await;
    let restored = h
        .app
        .queries()
        .get_session(&session)
        .await
        .unwrap()
        .unwrap();
    assert!(restored.capabilities.accept_task);
    assert_eq!(restored.capabilities, before.capabilities);
    let clients = h.app.clients();
    let target = pontia_agent_control::runtime::ControlTarget::resolve(
        &h.app.db(),
        &clients,
        &session,
        None,
    )
    .await
    .unwrap();
    let result = clients
        .get("acp")
        .unwrap()
        .session
        .as_ref()
        .unwrap()
        .resume(h.app.event_ingest_service(), &target)
        .await;
    assert!(
        matches!(result, Err(Error::CapabilityUnavailable(message)) if message == "ACP Agent first is not defined")
    );
    let mut clients = ClientRegistry::default();
    clients.register(pontia_client_acp::registration());
    let cold = AgentControl::builder(h.app.db(), h.root.path().into())
        .clients(clients)
        .build();
    let detail = cold.queries().get_session(&session).await.unwrap().unwrap();
    assert!(!detail.capabilities.accept_task);
    assert!(!detail.capabilities.timeline);
    assert_eq!(detail.client_details["acp"]["agent_id"], "first");
    assert_eq!(
        detail.client_details["acp"]["agent_info"]["name"],
        "fixture"
    );
    h.stop().await;
}

#[tokio::test]
async fn obsolete_saved_entries_do_not_hide_valid_creation_options() {
    let h = Harness::new(&[]).await;
    let records =
        pontia_storage_sqlite::repositories::agent_clients::SqliteAgentClientRepository::new(
            h.app.db(),
        );
    for key in ["first", "removed"] {
        records
            .save("acp", key, Some(true), Some(r#"{"command":"old"}"#))
            .await
            .unwrap();
    }
    let options = h.app.clients().creation_options(&h.app.db()).await.unwrap();
    let keys: Vec<_> = options
        .iter()
        .map(|entry| entry.metadata["acp_agent_id"].as_str().unwrap())
        .collect();
    assert_eq!(keys, ["second"]);
    assert!(h.create("first").await.is_err());
    assert!(h.create("removed").await.is_err());
    h.stop().await;
}
