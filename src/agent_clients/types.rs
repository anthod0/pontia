use crate::adapters::AdapterCapabilities;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatchBehavior {
    GenericTestAdapter,
    TmuxPaste,
    None,
}

pub type DispatchMode = DispatchBehavior;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadinessBehavior {
    RuntimeManagerImmediate,
    AgentClientEvent,
}

pub type ReadinessMode = ReadinessBehavior;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientSessionIdentityBehavior {
    RequiredOnReady,
    OptionalOnReady,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeBehavior {
    InProcessTest,
    Tmux(TmuxRuntimeBehavior),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TmuxRuntimeBehavior {
    pub command_env: Option<&'static str>,
    pub default_command: &'static str,
    pub session_identity_arg: Option<&'static str>,
    pub hook_log: Option<HookLogBehavior>,
    pub runtime_config_key: Option<&'static str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HookLogBehavior {
    pub env: &'static str,
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
    CurrentTurnFile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdapterEventBehavior {
    Disabled,
    JsonlOutbox { file_name: &'static str },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemPromptInjectionBehavior {
    Disabled,
    AppendFromExternalApi,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupHook {
    ClaudeCodeTrustWorkspace,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentClientSpec {
    pub client_type: &'static str,
    pub capabilities: AdapterCapabilities,
    pub runtime: RuntimeBehavior,
    pub dispatch: DispatchBehavior,
    pub readiness: ReadinessBehavior,
    pub client_session_identity: ClientSessionIdentityBehavior,
    pub interrupt: InterruptBehavior,
    pub terminate: TerminateBehavior,
    pub turn_context: TurnContextBehavior,
    pub adapter_events: AdapterEventBehavior,
    pub system_prompt_injection: SystemPromptInjectionBehavior,
    pub startup_hooks: &'static [StartupHook],
}

impl AgentClientSpec {
    pub fn tmux_runtime(&self) -> Option<TmuxRuntimeBehavior> {
        match self.runtime {
            RuntimeBehavior::Tmux(runtime) => Some(runtime),
            RuntimeBehavior::InProcessTest => None,
        }
    }
}
