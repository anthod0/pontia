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
            context_usage: ContextUsageCapability::Unsupported,
        }
    }

    pub fn pi_m0_default() -> Self {
        crate::pi::CAPABILITIES
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatchBehavior {
    InProcessRecorded,
    TmuxPaste,
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
    InProcess,
    Tmux(TmuxRuntimeBehavior),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TmuxRuntimeBehavior {
    pub command_env: Option<&'static str>,
    pub default_command: &'static str,
    pub startup_args: &'static [&'static str],
    pub session_identity_arg: Option<&'static str>,
    pub hook_log: Option<HookLogBehavior>,
    pub runtime_config_key: Option<&'static str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HookLogBehavior {
    pub file_name: &'static str,
    pub metadata_key: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterruptBehavior {
    Unsupported,
    TmuxInterrupt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminateBehavior {
    RuntimeManager,
    TmuxSendKeys(&'static [&'static str]),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnContextBehavior {
    Disabled,
    InternalApiClaim,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CurrentTurnIdBehavior {
    Include,
    Omit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnLifecycleBehavior {
    BackendManaged,
    ClientManagedForInteractiveTmux,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeBindingBehavior {
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
    Unsupported,
    PiJsonl,
}

/// Rust-side adapter strategy for one agent client.
///
/// These fields describe how the Rust backend starts, controls, observes, or
/// reads client-specific resources for the client. They intentionally do not
/// describe how a client extension internally reports facts through the
/// Internal Event API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentClientAdapter {
    pub runtime: RuntimeBehavior,
    pub dispatch: DispatchBehavior,
    pub client_session_identity: ClientSessionIdentityBehavior,
    pub interrupt: InterruptBehavior,
    pub terminate: TerminateBehavior,
    pub turn_context: TurnContextBehavior,
    pub current_turn_id: CurrentTurnIdBehavior,
    pub turn_lifecycle: TurnLifecycleBehavior,
    pub runtime_binding: RuntimeBindingBehavior,
    pub system_prompt_injection: SystemPromptInjectionBehavior,
    pub startup_hooks: &'static [StartupHook],
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
        self.adapter.dispatch == DispatchBehavior::TmuxPaste
            && self.adapter.turn_lifecycle == TurnLifecycleBehavior::ClientManagedForInteractiveTmux
    }

    pub fn owns_initial_tmux_turn(&self) -> bool {
        self.adapter.dispatch == DispatchBehavior::TmuxPaste
            && self.adapter.turn_lifecycle == TurnLifecycleBehavior::ClientManagedForInteractiveTmux
    }

    pub fn current_turn_context_includes_turn_id(&self) -> bool {
        self.adapter.current_turn_id == CurrentTurnIdBehavior::Include
    }

    pub fn runtime_binding_kind(&self) -> Option<&'static str> {
        match self.adapter.runtime_binding {
            RuntimeBindingBehavior::Unsupported => None,
            RuntimeBindingBehavior::Tmux { runtime_kind } => Some(runtime_kind),
        }
    }
}

impl AgentClientAdapter {
    pub fn tmux_runtime(&self) -> Option<TmuxRuntimeBehavior> {
        match self.runtime {
            RuntimeBehavior::Tmux(runtime) => Some(runtime),
            RuntimeBehavior::InProcess => None,
        }
    }
}
