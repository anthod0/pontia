#[test]
fn session_capabilities_reuses_agent_client_capabilities_schema() {
    let capabilities: pontia_application::SessionCapabilities =
        pontia_agent_clients::AgentClientCapabilities::pi_m0_default();

    assert!(capabilities.timeline);
    assert!(capabilities.topology);
    assert!(capabilities.branch_control);
    assert_eq!(
        capabilities.context_usage,
        pontia_agent_clients::ContextUsageCapability::Estimated
    );

    let agent_client_capabilities: pontia_agent_clients::AgentClientCapabilities = capabilities;
    assert!(agent_client_capabilities.accept_task);
}

#[test]
fn branch_control_is_independent_of_topology_observation() {
    let capabilities = pontia_agent_clients::AgentClientCapabilities {
        branch_control: true,
        topology: false,
        ..Default::default()
    };

    assert!(capabilities.branch_control);
    assert!(!capabilities.topology);
}
