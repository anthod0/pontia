use crate::{
    adapters::AdapterCapabilities,
    agent_clients::types::{
        AdapterEventBehavior, AgentClientSpec, DispatchBehavior, InterruptBehavior,
        ReadinessBehavior, RuntimeBehavior, SystemPromptInjectionBehavior, TurnContextBehavior,
    },
};

pub const CAPABILITIES: AdapterCapabilities = AdapterCapabilities {
    accept_task: true,
    report_turn_started: true,
    report_turn_finished: true,
    interrupt: false,
    stream_output: false,
    heartbeat: false,
    artifact_sources: false,
};

pub const SPEC: AgentClientSpec = AgentClientSpec {
    client_type: "generic",
    capabilities: CAPABILITIES,
    runtime: RuntimeBehavior::InProcessTest,
    dispatch: DispatchBehavior::GenericTestAdapter,
    readiness: ReadinessBehavior::RuntimeManagerImmediate,
    interrupt: InterruptBehavior::Unsupported,
    turn_context: TurnContextBehavior::Disabled,
    adapter_events: AdapterEventBehavior::Disabled,
    system_prompt_injection: SystemPromptInjectionBehavior::Disabled,
    startup_hooks: &[],
};
