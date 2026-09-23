mod channel;
mod data;
mod session;
pub use channel::{ClientControlChannel, ClientControlOperation};
pub use data::{
    BranchTargetRequest, ClientData, ClientLaunchRequest, ClientLauncher, NativeEventEvidence,
};
pub use session::{ClientOperation, ClientSession, ClientSessionDetails, InProcessClient};
#[cfg(any(test, feature = "generic-test-client"))]
mod generic_test;
pub mod raw_transcripts;
pub mod topology;
mod types;

#[cfg(any(test, feature = "generic-test-client"))]
pub use generic_test::{GenericTestClient, SPEC as TEST_SPEC, registration as test_registration};
pub use topology::{
    TopologyDiagnostic, TopologyResolution, TopologyResolveRequest, TopologyResolveResult,
    TurnTopologyCandidate, TurnTopologyResolver,
};
pub use types::{
    AgentClientAdapter, AgentClientCapabilities, AgentClientSpec, AgentInput,
    ClientSessionIdentityBehavior, ContextUsageCapability, DispatchBehavior, DispatchMode,
    HookLogBehavior, RuntimeBehavior, RuntimeBindingBehavior, TerminateBehavior,
    TmuxRuntimeBehavior, TurnLifecycleBehavior,
};

use raw_transcripts::{AgentBindingResolver, TimelineBoundaryCapturer, TurnTimelineReader};

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
