use pontia_application::client_contract::*;

pub const ACP_MAX_CAPABILITIES: AgentClientCapabilities = AgentClientCapabilities {
    accept_task: true,
    report_turn_started: true,
    report_turn_finished: true,
    interrupt: true,
    stream_output: true,
    heartbeat: false,
    timeline: false,
    topology: false,
    branch_control: false,
    list_models: false,
    set_model: false,
    manual_compaction: false,
    compaction_events: false,
    compaction_history: false,
    context_usage: ContextUsageCapability::Unsupported,
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
        native_turn_metadata_key: None,
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
