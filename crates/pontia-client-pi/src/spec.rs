use pontia_application::client_contract::{
    AgentClientAdapter, AgentClientCapabilities, AgentClientSpec, ClientSessionIdentityBehavior,
    ContextUsageCapability, DispatchBehavior, HookLogBehavior, RuntimeBehavior,
    RuntimeBindingBehavior, TerminateBehavior, TmuxRuntimeBehavior, TurnLifecycleBehavior,
};

pub const CAPABILITIES: AgentClientCapabilities = AgentClientCapabilities {
    accept_task: true,
    report_turn_started: true,
    report_turn_finished: true,
    interrupt: true,
    stream_output: true,
    heartbeat: false,
    timeline: true,
    topology: true,
    branch_control: true,
    list_models: true,
    set_model: true,
    context_usage: ContextUsageCapability::Estimated,
};

pub const SPEC: AgentClientSpec = AgentClientSpec {
    client_type: "pi",
    default_for_creation: true,
    capabilities: CAPABILITIES,
    adapter: AgentClientAdapter {
        lifecycle: pontia_application::client_contract::SessionLifecycleBehavior {
            coupled_runtime: true,
            confirmed_exit_recovery: true,
            restart_requires_exit: true,
            process_observation: Some(
                pontia_application::client_contract::ProcessObservationBehavior {
                    role: "tui",
                    observe_starting: false,
                    exit_event: pontia_application::PontiaEventType::SessionExited,
                    exit_reason: "agent_process_fingerprint_missing",
                },
            ),
            ..pontia_application::client_contract::SessionLifecycleBehavior::DEFAULT
        },
        native_turn_identity: false,
        native_turn_metadata_key: None,
        runtime: RuntimeBehavior::Tmux(TmuxRuntimeBehavior {
            process_names: &["pi"],
            hook_log: Some(HookLogBehavior {
                file_name: "pi-hook.log",
                metadata_key: "pi_hook_log",
            }),
        }),
        dispatch: DispatchBehavior::Connected,
        client_session_identity: ClientSessionIdentityBehavior::RequiredOnReady,
        terminate: TerminateBehavior::Connected,
        turn_lifecycle: TurnLifecycleBehavior::ClientManagedForInteractiveTmux,
        runtime_binding: RuntimeBindingBehavior::Tmux {
            runtime_kind: "pi_tui",
        },
    },
};
