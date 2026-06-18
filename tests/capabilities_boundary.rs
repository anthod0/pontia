#[test]
fn session_capabilities_reuses_agent_client_capabilities_schema() {
    let capabilities: pontia::application::SessionCapabilities =
        pontia_agent_clients::AgentClientCapabilities::pi_m0_default();

    assert!(capabilities.timeline);
    assert_eq!(
        capabilities.context_usage,
        pontia_agent_clients::ContextUsageCapability::Estimated
    );

    let agent_client_capabilities: pontia_agent_clients::AgentClientCapabilities = capabilities;
    assert!(agent_client_capabilities.accept_task);
}
