use crate::{
    adapters::AdapterCapabilities,
    agent_clients::types::{
        AdapterEventBehavior, AgentClientSpec, ClientSessionIdentityBehavior, DispatchBehavior,
        HookLogBehavior, InterruptBehavior, ReadinessBehavior, RuntimeBehavior,
        SystemPromptInjectionBehavior, TerminateBehavior, TmuxRuntimeBehavior, TurnContextBehavior,
    },
    application::ContextUsageCapability,
};

pub const CAPABILITIES: AdapterCapabilities = AdapterCapabilities {
    accept_task: true,
    report_turn_started: true,
    report_turn_finished: true,
    interrupt: true,
    stream_output: true,
    heartbeat: false,
    artifact_sources: true,
    context_usage: ContextUsageCapability::Estimated,
};

pub const SPEC: AgentClientSpec = AgentClientSpec {
    client_type: "pi",
    capabilities: CAPABILITIES,
    runtime: RuntimeBehavior::Tmux(TmuxRuntimeBehavior {
        command_env: Some("PONTIA_PI_TUI_COMMAND"),
        default_command: "pi --approve",
        session_identity_arg: Some("--session-id"),
        hook_log: Some(HookLogBehavior {
            env: "PONTIA_PI_HOOK_LOG",
            file_name: "pi-hook.log",
            metadata_key: "pi_hook_log",
        }),
        runtime_config_key: Some("pi"),
    }),
    dispatch: DispatchBehavior::TmuxPaste,
    readiness: ReadinessBehavior::AgentClientEvent,
    client_session_identity: ClientSessionIdentityBehavior::RequiredOnReady,
    interrupt: InterruptBehavior::TmuxInterrupt,
    terminate: TerminateBehavior::TmuxSendKeys(&["C-c", "C-c"]),
    turn_context: TurnContextBehavior::CurrentTurnFile,
    adapter_events: AdapterEventBehavior::JsonlOutbox {
        file_name: "adapter-events.jsonl",
    },
    system_prompt_injection: SystemPromptInjectionBehavior::AppendFromExternalApi,
    startup_hooks: &[],
};
