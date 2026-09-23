use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContextUsageCapability {
    #[default]
    Unsupported,
    Estimated,
    Exact,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentInput {
    pub session_id: String,
    pub dispatch_id: String,
    pub input: String,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentClientCapabilities {
    pub accept_task: bool,
    pub report_turn_started: bool,
    pub report_turn_finished: bool,
    pub interrupt: bool,
    pub stream_output: bool,
    pub heartbeat: bool,
    pub timeline: bool,
    pub topology: bool,
    pub branch_control: bool,
    pub list_models: bool,
    pub set_model: bool,
    pub context_usage: ContextUsageCapability,
}

impl AgentClientCapabilities {
    pub fn generic_default() -> Self {
        Self {
            accept_task: true,
            report_turn_started: true,
            report_turn_finished: true,
            interrupt: false,
            stream_output: false,
            heartbeat: false,
            timeline: false,
            topology: false,
            branch_control: false,
            list_models: false,
            set_model: false,
            context_usage: ContextUsageCapability::Unsupported,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatchBehavior {
    Connected,
    CodexProtocol,
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
    CodexAppServer,
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
    CodexArchive,
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
    CodexAppServer,
    Unsupported,
    Tmux { runtime_kind: &'static str },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemPromptInjectionBehavior {
    Disabled,
    AppendFromExternalApi,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupHook {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TranscriptBehavior {
    CodexRollout,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimelineSourceBehavior {
    Unsupported,
    Transcript,
}

/// Rust-side adapter strategy for one agent client.
///
/// These fields describe how the Rust backend starts, controls, observes, or
/// reads client-specific resources for the client. They intentionally do not
/// describe how a client extension reports facts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentClientAdapter {
    pub runtime: RuntimeBehavior,
    pub dispatch: DispatchBehavior,
    pub client_session_identity: ClientSessionIdentityBehavior,
    pub terminate: TerminateBehavior,
    pub turn_lifecycle: TurnLifecycleBehavior,
    pub runtime_binding: RuntimeBindingBehavior,
    pub system_prompt_injection: SystemPromptInjectionBehavior,
    pub startup_hooks: &'static [StartupHook],
    pub timeline_source: TimelineSourceBehavior,
    pub transcript: TranscriptBehavior,
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
    pub fn tmux_runtime(&self) -> Option<TmuxRuntimeBehavior> {
        self.adapter.tmux_runtime()
    }

    pub fn owns_interactive_tmux_turn(&self) -> bool {
        self.tmux_runtime().is_some()
            && self.adapter.turn_lifecycle == TurnLifecycleBehavior::ClientManagedForInteractiveTmux
    }

    pub fn owns_initial_tmux_turn(&self) -> bool {
        self.tmux_runtime().is_some()
            && self.adapter.turn_lifecycle == TurnLifecycleBehavior::ClientManagedForInteractiveTmux
    }

    pub fn runtime_binding_kind(&self) -> Option<&'static str> {
        match self.adapter.runtime_binding {
            RuntimeBindingBehavior::CodexAppServer => Some("codex_app_server"),
            RuntimeBindingBehavior::Unsupported => None,
            RuntimeBindingBehavior::Tmux { runtime_kind } => Some(runtime_kind),
        }
    }
}

impl AgentClientAdapter {
    pub fn tmux_runtime(&self) -> Option<TmuxRuntimeBehavior> {
        match self.runtime {
            RuntimeBehavior::Tmux(runtime) => Some(runtime),
            RuntimeBehavior::InProcess | RuntimeBehavior::CodexAppServer => None,
        }
    }
}
