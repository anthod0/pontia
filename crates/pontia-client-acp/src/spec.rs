use pontia_agent_control::client_contract::*;

pub const ACP_MAX_CAPABILITIES: AgentClientCapabilities = AgentClientCapabilities {
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
    fork_session: false,
    reasoning_effort_control: true,
    manual_compaction: true,
    compaction_events: true,
    compaction_history: true,
    context_usage: ContextUsageCapability::Exact,
};

pub const SPEC: AgentClientSpec = AgentClientSpec {
    client_type: "acp",
    default_for_creation: false,
    capabilities: ACP_MAX_CAPABILITIES,
    adapter: AgentClientAdapter {
        lifecycle: SessionLifecycleBehavior {
            execution_lifetime: pontia_core::domain::SessionExecutionLifetime::Subscription,
            ..SessionLifecycleBehavior::DEFAULT
        },
        native_turn_identity: false,
        runtime: RuntimeBehavior::External,
        control_runtime_role: Some("acp"),
        dispatch: DispatchBehavior::Connected,
        client_session_identity: ClientSessionIdentityBehavior::RequiredOnReady,
        terminate: TerminateBehavior::Connected,
        turn_lifecycle: TurnLifecycleBehavior::BackendManaged,
        runtime_binding: RuntimeBindingBehavior::Named {
            runtime_kind: "stdio",
        },
    },
};
