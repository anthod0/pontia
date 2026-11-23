use pontia_agent_control::client_contract::{
    AgentClientAdapter, AgentClientCapabilities, AgentClientSpec, ClientSessionIdentityBehavior,
    ContextUsageCapability, DispatchBehavior, HookLogBehavior, RuntimeBehavior,
    RuntimeBindingBehavior, TerminateBehavior, TmuxRuntimeBehavior, TurnLifecycleBehavior,
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
    topology: true,
    branch_control: true,
    fork_session: false,
    manual_compaction: true,
    compaction_events: true,
    compaction_history: true,
    context_usage: ContextUsageCapability::Estimated,
};

pub(crate) const CONTROL_RUNTIME_ROLE: &str = "pi_tui";

pub const SPEC: AgentClientSpec = AgentClientSpec {
    client_type: "pi",
    default_for_creation: true,
    capabilities: CAPABILITIES,
    adapter: AgentClientAdapter {
        lifecycle: pontia_agent_control::client_contract::SessionLifecycleBehavior {
            coupled_runtime: true,
            shutdown_requires_managed_launch: true,
            confirmed_exit_recovery: true,
            restart_requires_exit: true,
            process_observation: Some(
                pontia_agent_control::client_contract::ProcessObservationBehavior {
                    role: CONTROL_RUNTIME_ROLE,
                    observe_starting: false,
                    exit_event: pontia_agent_control::PontiaEventType::SessionExited,
                    exit_reason: "agent_process_fingerprint_missing",
                },
            ),
            ..pontia_agent_control::client_contract::SessionLifecycleBehavior::DEFAULT
        },
        native_turn_identity: false,
        native_turn_metadata_key: None,
        control_runtime_role: Some(CONTROL_RUNTIME_ROLE),
        runtime: RuntimeBehavior::Tmux(TmuxRuntimeBehavior {
            hook_log: Some(HookLogBehavior {
                file_name: "pi-hook.log",
                metadata_key: "pi_hook_log",
            }),
        }),
        dispatch: DispatchBehavior::Connected,
        client_session_identity: ClientSessionIdentityBehavior::RequiredOnReady,
        terminate: TerminateBehavior::Connected,
        turn_lifecycle: TurnLifecycleBehavior::ClientManagedInteractive,
        runtime_binding: RuntimeBindingBehavior::Named {
            runtime_kind: "pi_tui",
        },
    },
};
