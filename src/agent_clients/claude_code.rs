use crate::{
    adapters::AdapterCapabilities,
    agent_clients::types::{
        AdapterEventBehavior, AgentClientBackendSpec, AgentClientDefinition,
        ClientSessionIdentityBehavior, DispatchBehavior, HookLogBehavior, InterruptBehavior,
        ReadinessBehavior, RuntimeBehavior, StartupHook, SystemPromptInjectionBehavior,
        TerminateBehavior, TmuxRuntimeBehavior, TranscriptBehavior, TurnContextBehavior,
    },
    application::ContextUsageCapability,
};

pub const CAPABILITIES: AdapterCapabilities = AdapterCapabilities {
    accept_task: true,
    report_turn_started: false,
    report_turn_finished: true,
    interrupt: false,
    stream_output: false,
    heartbeat: false,
    artifact_sources: false,
    timeline: false,
    context_usage: ContextUsageCapability::Unsupported,
};

pub const DEFINITION: AgentClientDefinition = AgentClientDefinition {
    client_type: "claude_code",
    capabilities: CAPABILITIES,
    backend: AgentClientBackendSpec {
        runtime: RuntimeBehavior::Tmux(TmuxRuntimeBehavior {
            command_env: Some("PONTIA_CLAUDE_TUI_COMMAND"),
            default_command: "claude --dangerously-skip-permissions",
            session_identity_arg: None,
            hook_log: Some(HookLogBehavior {
                env: "PONTIA_CLAUDE_HOOK_LOG",
                file_name: "claude-hook.log",
                metadata_key: "claude_hook_log",
            }),
            runtime_config_key: Some("claude_code"),
        }),
        dispatch: DispatchBehavior::TmuxPaste,
        readiness: ReadinessBehavior::AgentClientEvent,
        client_session_identity: ClientSessionIdentityBehavior::OptionalOnReady,
        interrupt: InterruptBehavior::Unsupported,
        terminate: TerminateBehavior::TmuxSendKeys(&["C-c"]),
        turn_context: TurnContextBehavior::CurrentTurnFile,
        adapter_events: AdapterEventBehavior::Disabled,
        system_prompt_injection: SystemPromptInjectionBehavior::Disabled,
        startup_hooks: &[StartupHook::ClaudeCodeTrustWorkspace],
        transcript: TranscriptBehavior::Unsupported,
    },
};
