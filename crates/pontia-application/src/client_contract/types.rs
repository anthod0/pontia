pub use pontia_core::client_capabilities::{
    AgentClientCapabilities, AgentInput, ContextUsageCapability,
};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatchBehavior {
    Connected,
    InProcessRecorded,
    None,
}

pub type DispatchMode = DispatchBehavior;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientSessionIdentityBehavior {
    RequiredOnReady,
    OptionalOnReady,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeBehavior {
    External,
    InProcess,
    Tmux(TmuxRuntimeBehavior),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TmuxRuntimeBehavior {
    /// Process names used to identify this agent below its bound tmux pane.
    pub process_names: &'static [&'static str],
    pub hook_log: Option<HookLogBehavior>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HookLogBehavior {
    pub file_name: &'static str,
    pub metadata_key: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminateBehavior {
    RuntimeManager,
    Connected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnLifecycleBehavior {
    ClientManaged,
    BackendManaged,
    ClientManagedForInteractiveTmux,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeBindingBehavior {
    Named { runtime_kind: &'static str },
    Unsupported,
    Tmux { runtime_kind: &'static str },
}

/// Rust-side adapter strategy for one agent client.
///
/// These fields describe how the Rust backend starts, controls, observes, or
/// reads client-specific resources for the client. They intentionally do not
/// describe how a client extension reports facts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentClientAdapter {
    pub native_turn_identity: bool,
    pub runtime: RuntimeBehavior,
    pub dispatch: DispatchBehavior,
    pub client_session_identity: ClientSessionIdentityBehavior,
    pub terminate: TerminateBehavior,
    pub turn_lifecycle: TurnLifecycleBehavior,
    pub runtime_binding: RuntimeBindingBehavior,
}

/// Complete static spec for an agent client.
///
/// `capabilities` answers "can this client/session support this feature?";
/// `adapter` answers "when pontia's Rust backend owns the implementation, how
/// does it do it?" Extension-internal implementation details live in
/// `clients/*`, not in this spec.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentClientSpec {
    pub client_type: &'static str,
    pub capabilities: AgentClientCapabilities,
    pub adapter: AgentClientAdapter,
}

impl AgentClientSpec {
    pub fn launch_options(&self) -> pontia_runtime::TmuxLaunchOptions {
        pontia_runtime::TmuxLaunchOptions {
            capabilities: self.capabilities.clone(),
            hook_log: self
                .tmux_runtime()
                .and_then(|runtime| runtime.hook_log)
                .map(|log| (log.file_name, log.metadata_key)),
        }
    }

    pub fn tmux_runtime(&self) -> Option<TmuxRuntimeBehavior> {
        self.adapter.tmux_runtime()
    }

    pub fn owns_interactive_tmux_turn(&self) -> bool {
        self.tmux_runtime().is_some()
            && self.adapter.turn_lifecycle == TurnLifecycleBehavior::ClientManagedForInteractiveTmux
    }

    pub fn runtime_binding_kind(&self) -> Option<&'static str> {
        match self.adapter.runtime_binding {
            RuntimeBindingBehavior::Named { runtime_kind } => Some(runtime_kind),
            RuntimeBindingBehavior::Unsupported => None,
            RuntimeBindingBehavior::Tmux { runtime_kind } => Some(runtime_kind),
        }
    }
}

impl AgentClientAdapter {
    pub fn tmux_runtime(&self) -> Option<TmuxRuntimeBehavior> {
        match self.runtime {
            RuntimeBehavior::Tmux(runtime) => Some(runtime),
            RuntimeBehavior::InProcess | RuntimeBehavior::External => None,
        }
    }
}
