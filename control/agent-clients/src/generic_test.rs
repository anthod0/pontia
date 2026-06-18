use std::sync::{Mutex, OnceLock};

use crate::{
    AgentClientCapabilities, AgentInput, ContextUsageCapability,
    types::{
        AdapterEventBehavior, AgentClientAdapter, AgentClientSpec, ClientSessionIdentityBehavior,
        CurrentTurnIdBehavior, DispatchBehavior, InterruptBehavior, ReadinessBehavior,
        RuntimeBehavior, RuntimeBindingBehavior, SystemPromptInjectionBehavior, TerminateBehavior,
        TranscriptBehavior, TurnContextBehavior, TurnLifecycleBehavior,
    },
};

pub const CAPABILITIES: AgentClientCapabilities = AgentClientCapabilities {
    accept_task: true,
    report_turn_started: true,
    report_turn_finished: true,
    interrupt: false,
    stream_output: false,
    heartbeat: false,
    artifact_sources: false,
    timeline: false,
    context_usage: ContextUsageCapability::Unsupported,
};

pub const SPEC: AgentClientSpec = AgentClientSpec {
    client_type: "generic",
    capabilities: CAPABILITIES,
    adapter: AgentClientAdapter {
        runtime: RuntimeBehavior::InProcess,
        dispatch: DispatchBehavior::InProcessRecorded,
        readiness: ReadinessBehavior::RuntimeManagerImmediate,
        client_session_identity: ClientSessionIdentityBehavior::Unsupported,
        interrupt: InterruptBehavior::Unsupported,
        terminate: TerminateBehavior::RuntimeManager,
        turn_context: TurnContextBehavior::Disabled,
        current_turn_id: CurrentTurnIdBehavior::Include,
        turn_lifecycle: TurnLifecycleBehavior::BackendManaged,
        runtime_binding: RuntimeBindingBehavior::Unsupported,
        adapter_events: AdapterEventBehavior::Disabled,
        system_prompt_injection: SystemPromptInjectionBehavior::Disabled,
        startup_hooks: &[],
        transcript: TranscriptBehavior::Unsupported,
    },
};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct InProcessRecordedDispatchBehavior {
    pub auto_start_turn: bool,
    pub write_current_turn_context: bool,
}

#[derive(Debug, Default, Clone)]
pub struct GenericTestClient;

impl GenericTestClient {
    pub fn clear_recorded_inputs() {
        recorded_inputs()
            .lock()
            .expect("recorded inputs lock")
            .clear();
        *test_capabilities().lock().expect("test capabilities lock") =
            AgentClientCapabilities::generic_default();
        *test_behavior().lock().expect("test behavior lock") =
            InProcessRecordedDispatchBehavior::default();
    }

    pub fn set_capabilities(capabilities: AgentClientCapabilities) {
        *test_capabilities().lock().expect("test capabilities lock") = capabilities;
    }

    pub fn set_behavior(behavior: InProcessRecordedDispatchBehavior) {
        *test_behavior().lock().expect("test behavior lock") = behavior;
    }

    pub fn behavior() -> InProcessRecordedDispatchBehavior {
        test_behavior().lock().expect("test behavior lock").clone()
    }

    pub fn recorded_inputs() -> Vec<AgentInput> {
        recorded_inputs()
            .lock()
            .expect("recorded inputs lock")
            .clone()
    }
}

impl GenericTestClient {
    pub fn accept_input(&self, input: AgentInput) -> pontia_core::Result<()> {
        recorded_inputs()
            .lock()
            .expect("recorded inputs lock")
            .push(input);
        Ok(())
    }

    pub fn capabilities(&self) -> AgentClientCapabilities {
        test_capabilities()
            .lock()
            .expect("test capabilities lock")
            .clone()
    }
}

fn recorded_inputs() -> &'static Mutex<Vec<AgentInput>> {
    static RECORDED_INPUTS: OnceLock<Mutex<Vec<AgentInput>>> = OnceLock::new();
    RECORDED_INPUTS.get_or_init(|| Mutex::new(Vec::new()))
}

fn test_capabilities() -> &'static Mutex<AgentClientCapabilities> {
    static TEST_CAPABILITIES: OnceLock<Mutex<AgentClientCapabilities>> = OnceLock::new();
    TEST_CAPABILITIES.get_or_init(|| Mutex::new(AgentClientCapabilities::generic_default()))
}

fn test_behavior() -> &'static Mutex<InProcessRecordedDispatchBehavior> {
    static TEST_BEHAVIOR: OnceLock<Mutex<InProcessRecordedDispatchBehavior>> = OnceLock::new();
    TEST_BEHAVIOR.get_or_init(|| Mutex::new(InProcessRecordedDispatchBehavior::default()))
}
