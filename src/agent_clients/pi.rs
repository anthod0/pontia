use crate::{
    adapters::AdapterCapabilities,
    agent_clients::types::{
        AdapterEventBehavior, AgentClientSpec, DispatchBehavior, HookLogBehavior,
        InterruptBehavior, ReadinessBehavior, RuntimeBehavior, SystemPromptInjectionBehavior,
        TmuxRuntimeBehavior, TurnContextBehavior,
    },
};

pub const CAPABILITIES: AdapterCapabilities = AdapterCapabilities {
    accept_task: true,
    report_turn_started: true,
    report_turn_finished: true,
    interrupt: true,
    stream_output: true,
    heartbeat: false,
    artifact_sources: true,
};

pub const SPEC: AgentClientSpec = AgentClientSpec {
    client_type: "pi",
    capabilities: CAPABILITIES,
    runtime: RuntimeBehavior::Tmux(TmuxRuntimeBehavior {
        command_env: Some("PILOTFY_PI_TUI_COMMAND"),
        default_command: "pi",
        session_identity_arg: Some("--session-id"),
        hook_log: Some(HookLogBehavior {
            env: "PILOTFY_PI_HOOK_LOG",
            file_name: "pi-hook.log",
            metadata_key: "pi_hook_log",
        }),
        runtime_config_key: Some("pi"),
    }),
    dispatch: DispatchBehavior::TmuxPaste,
    readiness: ReadinessBehavior::AgentClientEvent,
    interrupt: InterruptBehavior::TmuxInterrupt,
    turn_context: TurnContextBehavior::CurrentTurnFile,
    adapter_events: AdapterEventBehavior::JsonlOutbox {
        file_name: "adapter-events.jsonl",
    },
    system_prompt_injection: SystemPromptInjectionBehavior::AppendFromExternalApi,
    startup_hooks: &[],
};
