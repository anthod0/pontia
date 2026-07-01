use crate::{
    AgentClientCapabilities, ContextUsageCapability,
    types::{
        AgentClientAdapter, AgentClientSpec, ClientSessionIdentityBehavior, CurrentTurnIdBehavior,
        DispatchBehavior, HookLogBehavior, InterruptBehavior, ReadinessBehavior, RuntimeBehavior,
        RuntimeBindingBehavior, SystemPromptInjectionBehavior, TerminateBehavior,
        TmuxRuntimeBehavior, TranscriptBehavior, TurnContextBehavior, TurnLifecycleBehavior,
    },
};

pub const CAPABILITIES: AgentClientCapabilities = AgentClientCapabilities {
    accept_task: true,
    report_turn_started: true,
    report_turn_finished: true,
    interrupt: true,
    stream_output: false,
    heartbeat: false,
    timeline: false,
    context_usage: ContextUsageCapability::Unsupported,
};

pub const SPEC: AgentClientSpec = AgentClientSpec {
    client_type: "claude",
    capabilities: CAPABILITIES,
    adapter: AgentClientAdapter {
        runtime: RuntimeBehavior::Tmux(TmuxRuntimeBehavior {
            command_env: Some("PONTIA_CLAUDE_TUI_COMMAND"),
            default_command: "claude",
            startup_args: &[],
            session_identity_arg: None,
            hook_log: Some(HookLogBehavior {
                file_name: "claude-hook.log",
                metadata_key: "claude_hook_log",
            }),
            runtime_config_key: Some("claude"),
        }),
        dispatch: DispatchBehavior::TmuxPaste,
        readiness: ReadinessBehavior::AgentClientEvent,
        client_session_identity: ClientSessionIdentityBehavior::RequiredOnReady,
        interrupt: InterruptBehavior::TmuxInterrupt,
        terminate: TerminateBehavior::TmuxSendKeys(&["C-c", "C-c"]),
        turn_context: TurnContextBehavior::InternalApiClaim,
        current_turn_id: CurrentTurnIdBehavior::Omit,
        turn_lifecycle: TurnLifecycleBehavior::ClientManagedForInteractiveTmux,
        runtime_binding: RuntimeBindingBehavior::Tmux {
            runtime_kind: "claude_tui",
        },
        system_prompt_injection: SystemPromptInjectionBehavior::Disabled,
        startup_hooks: &[],
        transcript: TranscriptBehavior::Unsupported,
    },
};
