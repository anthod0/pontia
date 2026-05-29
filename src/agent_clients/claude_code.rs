use crate::{
    adapters::AdapterCapabilities,
    agent_clients::types::{
        AdapterEventBehavior, AgentClientSpec, DispatchBehavior, HookLogBehavior,
        InterruptBehavior, ReadinessBehavior, RuntimeBehavior, StartupHook,
        SystemPromptInjectionBehavior, TmuxRuntimeBehavior, TurnContextBehavior,
    },
};

pub const CAPABILITIES: AdapterCapabilities = AdapterCapabilities {
    accept_task: true,
    report_turn_started: false,
    report_turn_finished: true,
    interrupt: false,
    stream_output: false,
    heartbeat: false,
    artifact_sources: false,
};

pub const SPEC: AgentClientSpec = AgentClientSpec {
    client_type: "claude_code",
    capabilities: CAPABILITIES,
    runtime: RuntimeBehavior::Tmux(TmuxRuntimeBehavior {
        command_env: Some("LLMPARTY_CLAUDE_TUI_COMMAND"),
        default_command: "claude --dangerously-skip-permissions",
        session_identity_arg: None,
        hook_log: Some(HookLogBehavior {
            env: "LLMPARTY_CLAUDE_HOOK_LOG",
            file_name: "claude-hook.log",
            metadata_key: "claude_hook_log",
        }),
        runtime_config_key: Some("claude_code"),
    }),
    dispatch: DispatchBehavior::TmuxPaste,
    readiness: ReadinessBehavior::AgentClientEvent,
    interrupt: InterruptBehavior::Unsupported,
    turn_context: TurnContextBehavior::CurrentTurnFile,
    adapter_events: AdapterEventBehavior::Disabled,
    system_prompt_injection: SystemPromptInjectionBehavior::Disabled,
    startup_hooks: &[StartupHook::ClaudeCodeTrustWorkspace],
};
