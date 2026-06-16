use crate::{
    adapters::AdapterCapabilities,
    agent_clients::types::{
        AdapterEventBehavior, AgentClientSpec, ClientSessionIdentityBehavior, DispatchBehavior,
        InterruptBehavior, ReadinessBehavior, RuntimeBehavior, SystemPromptInjectionBehavior,
        TerminateBehavior, TurnContextBehavior,
    },
    application::ContextUsageCapability,
};

pub const CAPABILITIES: AdapterCapabilities = AdapterCapabilities {
    accept_task: true,
    report_turn_started: true,
    report_turn_finished: true,
    interrupt: false,
    stream_output: false,
    heartbeat: false,
    artifact_sources: false,
    context_usage: ContextUsageCapability::Unsupported,
};

pub const SPEC: AgentClientSpec = AgentClientSpec {
    client_type: "generic",
    capabilities: CAPABILITIES,
    runtime: RuntimeBehavior::InProcessTest,
    dispatch: DispatchBehavior::GenericTestAdapter,
    readiness: ReadinessBehavior::RuntimeManagerImmediate,
    client_session_identity: ClientSessionIdentityBehavior::Unsupported,
    interrupt: InterruptBehavior::Unsupported,
    terminate: TerminateBehavior::RuntimeManager,
    turn_context: TurnContextBehavior::Disabled,
    adapter_events: AdapterEventBehavior::Disabled,
    system_prompt_injection: SystemPromptInjectionBehavior::Disabled,
    startup_hooks: &[],
};
