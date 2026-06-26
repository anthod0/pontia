pub mod raw_transcripts;

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
    stream_output: true,
    heartbeat: false,
    artifact_sources: true,
    timeline: true,
    context_usage: ContextUsageCapability::Estimated,
};

pub const SPEC: AgentClientSpec = AgentClientSpec {
    client_type: "pi",
    capabilities: CAPABILITIES,
    adapter: AgentClientAdapter {
        runtime: RuntimeBehavior::Tmux(TmuxRuntimeBehavior {
            command_env: Some("PONTIA_PI_TUI_COMMAND"),
            default_command: "pi --approve",
            session_identity_arg: Some("--session-id"),
            hook_log: Some(HookLogBehavior {
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
        turn_context: TurnContextBehavior::InternalApiClaim,
        current_turn_id: CurrentTurnIdBehavior::Omit,
        turn_lifecycle: TurnLifecycleBehavior::ClientManagedForInteractiveTmux,
        runtime_binding: RuntimeBindingBehavior::Tmux {
            runtime_kind: "pi_tui",
        },
        system_prompt_injection: SystemPromptInjectionBehavior::AppendFromExternalApi,
        startup_hooks: &[],
        transcript: TranscriptBehavior::PiJsonl,
    },
};
