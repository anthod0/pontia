#[cfg(any(test, feature = "generic-test-client"))]
mod generic_test;
pub mod pi;
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
    ClientSessionIdentityBehavior, ContextUsageCapability, CurrentTurnIdBehavior, DispatchBehavior,
    DispatchMode, HookLogBehavior, InterruptBehavior, RuntimeBehavior, RuntimeBindingBehavior,
    StartupHook, SystemPromptInjectionBehavior, TerminateBehavior, TimelineSourceBehavior,
    TmuxRuntimeBehavior, TranscriptBehavior, TurnContextBehavior, TurnLifecycleBehavior,
};

use raw_transcripts::{AgentBindingResolver, TimelineBoundaryCapturer, TurnTimelineReader};

pub const AGENT_CLIENTS: &[AgentClientSpec] = &[
    #[cfg(any(test, feature = "generic-test-client"))]
    generic_test::SPEC,
    pi::SPEC,
];

pub fn default_real_client_spec() -> &'static AgentClientSpec {
    &pi::SPEC
}

pub fn default_real_client_type() -> &'static str {
    default_real_client_spec().client_type
}

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

pub fn topology_backend_for(client_type: &str) -> Option<TurnTopologyBackend> {
    let spec = get_client_spec(client_type)?;
    if !spec.capabilities.topology {
        return None;
    }
    match client_type {
        "pi" => Some(TurnTopologyBackend {
            resolver: Box::new(pi::topology::PiTopologyResolver::new()),
        }),
        _ => None,
    }
}

pub fn timeline_boundary_backend_for(client_type: &str) -> Option<TimelineBoundaryBackend> {
    let spec = get_client_spec(client_type)?;
    match spec.adapter.transcript {
        TranscriptBehavior::Unsupported => None,
        TranscriptBehavior::PiJsonl => Some(TimelineBoundaryBackend {
            resolver: Box::new(pi::raw_transcripts::PiAgentBindingResolver::new()),
            capturer: Box::new(pi::raw_transcripts::PiTimelineAdapter::new()),
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
        TranscriptBehavior::PiJsonl => Some(TurnTimelineBackend {
            resolver: Box::new(pi::raw_transcripts::PiAgentBindingResolver::new()),
            reader: Box::new(pi::raw_transcripts::PiTimelineAdapter::new()),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_knows_builtin_clients_and_rejects_unknown() {
        assert!(is_supported_client_type("generic"));
        assert!(is_supported_client_type("pi"));
        assert!(!is_supported_client_type("unsupported"));
    }

    #[test]
    fn topology_capability_is_enabled_only_for_clients_with_an_adapter() {
        assert!(
            get_client_spec("pi")
                .expect("pi client spec")
                .capabilities
                .topology
        );
        assert!(topology_backend_for("pi").is_some());
        let generic = get_client_spec("generic").expect("generic test client spec");
        assert!(!generic.capabilities.topology);
        assert!(topology_backend_for("generic").is_none());
    }
}
