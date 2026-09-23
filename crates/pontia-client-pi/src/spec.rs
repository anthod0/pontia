use pontia_agent_clients::{
    AgentClientAdapter, AgentClientCapabilities, AgentClientSpec, ClientSessionIdentityBehavior,
    ContextUsageCapability, DispatchBehavior, HookLogBehavior, RuntimeBehavior,
    RuntimeBindingBehavior, SystemPromptInjectionBehavior, TerminateBehavior,
    TimelineSourceBehavior, TmuxRuntimeBehavior, TranscriptBehavior, TurnLifecycleBehavior,
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
    capabilities: CAPABILITIES,
    adapter: AgentClientAdapter {
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
        system_prompt_injection: SystemPromptInjectionBehavior::AppendFromExternalApi,
        startup_hooks: &[],
        timeline_source: TimelineSourceBehavior::Transcript,
        transcript: TranscriptBehavior::Unsupported,
    },
};
