use pontia_application::client_contract::{
    AgentClientAdapter, AgentClientCapabilities, AgentClientSpec, ClientSessionIdentityBehavior,
    ContextUsageCapability, DispatchBehavior, RuntimeBehavior, RuntimeBindingBehavior,
    TerminateBehavior, TurnLifecycleBehavior,
};

pub const CAPABILITIES: AgentClientCapabilities = AgentClientCapabilities {
    accept_task: true,
    report_turn_started: true,
    report_turn_finished: true,
    interrupt: true,
    stream_output: false,
    heartbeat: false,
    timeline: false,
    topology: false,
    branch_control: false,
    list_models: true,
    set_model: true,
    context_usage: ContextUsageCapability::Unsupported,
};

pub const SPEC: AgentClientSpec = AgentClientSpec {
    client_type: "codex",
    default_for_creation: false,
    capabilities: CAPABILITIES,
    adapter: AgentClientAdapter {
        lifecycle: pontia_application::client_contract::SessionLifecycleBehavior {
            execution_lifetime: pontia_core::domain::SessionExecutionLifetime::Subscription,
            independent_interface: true,
            process_observation: Some(
                pontia_application::client_contract::ProcessObservationBehavior {
                    role: "interface",
                    observe_starting: true,
                    exit_event: pontia_application::PontiaEventType::RuntimeExited,
                    exit_reason: "interface_process_fingerprint_missing",
                },
            ),
            ..pontia_application::client_contract::SessionLifecycleBehavior::DEFAULT
        },
        native_turn_identity: true,
        native_turn_metadata_key: Some("codex_turn_id"),
        runtime: RuntimeBehavior::External,
        dispatch: DispatchBehavior::Connected,
        client_session_identity: ClientSessionIdentityBehavior::RequiredOnReady,
        terminate: TerminateBehavior::Connected,
        turn_lifecycle: TurnLifecycleBehavior::ClientManaged,
        runtime_binding: RuntimeBindingBehavior::SharedBackend,
    },
};
