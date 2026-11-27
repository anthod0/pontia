use pontia_agent_control::client_contract::{
    AgentClientAdapter, AgentClientCapabilities, AgentClientSpec, ClientSessionIdentityBehavior,
    ContextUsageCapability, DispatchBehavior, RuntimeBehavior, RuntimeBindingBehavior,
    TerminateBehavior, TurnLifecycleBehavior,
};

pub const CAPABILITIES: AgentClientCapabilities = AgentClientCapabilities {
    accept_task: true,
    steer: true,
    report_turn_started: true,
    report_turn_finished: true,
    interrupt: true,
    stream_output: true,
    heartbeat: false,
    timeline: true,
    topology: false,
    branch_control: false,
    fork_session: true,
    reasoning_effort_control: true,
    manual_compaction: true,
    compaction_events: true,
    compaction_history: true,
    context_usage: ContextUsageCapability::Estimated,
};

pub const SPEC: AgentClientSpec = AgentClientSpec {
    client_type: "codex",
    default_for_creation: false,
    capabilities: CAPABILITIES,
    adapter: AgentClientAdapter {
        lifecycle: pontia_agent_control::client_contract::SessionLifecycleBehavior {
            execution_lifetime: pontia_core::domain::SessionExecutionLifetime::Subscription,
            ..pontia_agent_control::client_contract::SessionLifecycleBehavior::DEFAULT
        },
        native_turn_identity: true,
        runtime: RuntimeBehavior::External,
        control_runtime_role: None,
        dispatch: DispatchBehavior::Connected,
        client_session_identity: ClientSessionIdentityBehavior::RequiredOnReady,
        terminate: TerminateBehavior::Connected,
        turn_lifecycle: TurnLifecycleBehavior::ClientManaged,
        runtime_binding: RuntimeBindingBehavior::SharedBackend,
    },
};
