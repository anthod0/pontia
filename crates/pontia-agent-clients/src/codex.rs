use crate::types::*;
pub mod rollout;

pub const SUPPORTED_VERSION: &str = "0.155.1";

pub const CAPABILITIES: AgentClientCapabilities = AgentClientCapabilities {
    accept_task: true,
    report_turn_started: true,
    report_turn_finished: true,
    interrupt: true,
    stream_output: false,
    heartbeat: false,
    timeline: false,
    topology: false,
    branch_control: false,
    list_models: true,
    set_model: true,
    context_usage: ContextUsageCapability::Unsupported,
};

pub const SPEC: AgentClientSpec = AgentClientSpec {
    client_type: "codex",
    capabilities: CAPABILITIES,
    adapter: AgentClientAdapter {
        runtime: RuntimeBehavior::CodexAppServer,
        dispatch: DispatchBehavior::CodexProtocol,
        client_session_identity: ClientSessionIdentityBehavior::RequiredOnReady,
        interrupt: InterruptBehavior::CodexProtocol,
        terminate: TerminateBehavior::CodexArchive,
        turn_context: TurnContextBehavior::Disabled,
        current_turn_id: CurrentTurnIdBehavior::Omit,
        turn_lifecycle: TurnLifecycleBehavior::ClientManaged,
        runtime_binding: RuntimeBindingBehavior::CodexAppServer,
        system_prompt_injection: SystemPromptInjectionBehavior::Disabled,
        startup_hooks: &[],
        timeline_source: TimelineSourceBehavior::Transcript,
        transcript: TranscriptBehavior::CodexRollout,
    },
};
