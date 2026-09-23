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
    capabilities: CAPABILITIES,
    adapter: AgentClientAdapter {
        native_turn_identity: true,
        runtime: RuntimeBehavior::External,
        dispatch: DispatchBehavior::Connected,
        client_session_identity: ClientSessionIdentityBehavior::RequiredOnReady,
        terminate: TerminateBehavior::Connected,
        turn_lifecycle: TurnLifecycleBehavior::ClientManaged,
        runtime_binding: RuntimeBindingBehavior::Named {
            runtime_kind: "codex_app_server",
        },
    },
};
