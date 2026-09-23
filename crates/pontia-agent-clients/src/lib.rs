pub mod codex;
#[cfg(any(test, feature = "generic-test-client"))]
mod generic_test;
pub mod raw_transcripts;
pub mod topology;
mod types;

#[cfg(any(test, feature = "generic-test-client"))]
pub use generic_test::GenericTestClient;
pub use topology::{
    TopologyDiagnostic, TopologyResolution, TopologyResolveRequest, TopologyResolveResult,
    TurnTopologyCandidate, TurnTopologyResolver,
};
pub use types::{
    AgentClientAdapter, AgentClientCapabilities, AgentClientSpec, AgentInput,
    ClientSessionIdentityBehavior, ContextUsageCapability, DispatchBehavior, DispatchMode,
    HookLogBehavior, RuntimeBehavior, RuntimeBindingBehavior, StartupHook,
    SystemPromptInjectionBehavior, TerminateBehavior, TimelineSourceBehavior, TmuxRuntimeBehavior,
    TranscriptBehavior, TurnLifecycleBehavior,
};

use raw_transcripts::{AgentBindingResolver, TimelineBoundaryCapturer, TurnTimelineReader};

pub const AGENT_CLIENTS: &[AgentClientSpec] = &[
    #[cfg(any(test, feature = "generic-test-client"))]
    generic_test::SPEC,
    codex::SPEC,
];

pub fn client_session_identity_required_on_ready(client_type: &str) -> bool {
    get_client_spec(client_type).is_some_and(|spec| {
        spec.adapter.client_session_identity == ClientSessionIdentityBehavior::RequiredOnReady
    })
}

pub struct TimelineBoundaryBackend {
    pub resolver: Box<dyn AgentBindingResolver + Send + Sync>,
    pub capturer: Box<dyn TimelineBoundaryCapturer + Send + Sync>,
}

pub struct TurnTimelineBackend {
    pub resolver: Box<dyn AgentBindingResolver + Send + Sync>,
    pub reader: Box<dyn TurnTimelineReader + Send + Sync>,
}

pub struct TurnTopologyBackend {
    pub resolver: Box<dyn TurnTopologyResolver + Send + Sync>,
}

pub fn timeline_boundary_backend_for(client_type: &str) -> Option<TimelineBoundaryBackend> {
    let spec = get_client_spec(client_type)?;
    match spec.adapter.transcript {
        TranscriptBehavior::Unsupported => None,
        TranscriptBehavior::CodexRollout => Some(TimelineBoundaryBackend {
            resolver: Box::new(codex::rollout::CodexRollout),
            capturer: Box::new(codex::rollout::CodexRollout),
        }),
    }
}

pub fn turn_timeline_backend_for(client_type: &str) -> Option<TurnTimelineBackend> {
    let spec = get_client_spec(client_type)?;
    if spec.adapter.timeline_source != TimelineSourceBehavior::Transcript {
        return None;
    }
    match spec.adapter.transcript {
        TranscriptBehavior::Unsupported => None,
        TranscriptBehavior::CodexRollout => Some(TurnTimelineBackend {
            resolver: Box::new(codex::rollout::CodexRollout),
            reader: Box::new(codex::rollout::CodexRollout),
        }),
    }
}

pub fn in_process_capabilities(client_type: &str) -> Option<AgentClientCapabilities> {
    #[cfg(any(test, feature = "generic-test-client"))]
    if client_type == "generic" {
        return Some(GenericTestClient.capabilities());
    }
    let _ = client_type;
    None
}

pub fn in_process_ready_event(
    client_type: &str,
    session_id: &str,
    runtime_instance_id: &str,
) -> Option<pontia_core::domain::ReportedEvent> {
    #[cfg(any(test, feature = "generic-test-client"))]
    if client_type == "generic" {
        return Some(GenericTestClient::ready_event(
            session_id,
            runtime_instance_id,
        ));
    }
    let _ = (client_type, session_id, runtime_instance_id);
    None
}

pub fn accept_in_process_input(client_type: &str, input: AgentInput) -> pontia_core::Result<()> {
    #[cfg(any(test, feature = "generic-test-client"))]
    if client_type == "generic" {
        return GenericTestClient.accept_input(input);
    }
    let _ = input;
    Err(pontia_core::error::Error::Domain(format!(
        "{client_type} does not support in-process input dispatch"
    )))
}

pub fn run_startup_hooks(
    hooks: &[StartupHook],
    _workspace: &std::path::Path,
) -> pontia_core::Result<()> {
    match hooks {
        [] => Ok(()),
        [hook, ..] => match *hook {},
    }
}

pub fn get_client_spec(client_type: &str) -> Option<&'static AgentClientSpec> {
    AGENT_CLIENTS
        .iter()
        .find(|client| client.client_type == client_type)
}

pub fn is_supported_client_type(client_type: &str) -> bool {
    get_client_spec(client_type).is_some()
}
