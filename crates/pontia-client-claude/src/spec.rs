use pontia_application::client_contract::*;

pub const SPEC: AgentClientSpec = AgentClientSpec {
    client_type: "claude-agent",
    default_for_creation: false,
    capabilities: AgentClientCapabilities {
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
    },
    adapter: AgentClientAdapter {
        lifecycle: SessionLifecycleBehavior::DEFAULT,
        native_turn_identity: false,
        native_turn_metadata_key: None,
        runtime: RuntimeBehavior::External,
        control_runtime_role: Some("claude_sdk_host"),
        dispatch: DispatchBehavior::Connected,
        client_session_identity: ClientSessionIdentityBehavior::RequiredOnReady,
        terminate: TerminateBehavior::Connected,
        turn_lifecycle: TurnLifecycleBehavior::ClientManaged,
        runtime_binding: RuntimeBindingBehavior::Named {
            runtime_kind: "unix_socket",
        },
    },
};
