use pontia_core::client_capabilities::AgentClientCapabilities;

#[test]
fn reasoning_effort_control_defaults_to_unavailable_and_intersects() {
    assert!(!AgentClientCapabilities::default().reasoning_effort_control);
    assert!(!AgentClientCapabilities::generic_default().reasoning_effort_control);

    let supported = AgentClientCapabilities {
        reasoning_effort_control: true,
        ..Default::default()
    };
    assert!(supported.intersection(&supported).reasoning_effort_control);
    assert!(
        !supported
            .intersection(&AgentClientCapabilities::default())
            .reasoning_effort_control
    );
}
