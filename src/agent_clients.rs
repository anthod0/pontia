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
    pub interrupt: InterruptBehavior,
    pub turn_context: TurnContextBehavior,
    pub adapter_events: AdapterEventBehavior,
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

const GENERIC_CAPABILITIES: AdapterCapabilities = AdapterCapabilities {
    accept_task: true,
    report_turn_started: true,
    report_turn_finished: true,
    interrupt: false,
    stream_output: false,
    heartbeat: false,
    artifact_sources: false,
};

const PI_CAPABILITIES: AdapterCapabilities = AdapterCapabilities {
    accept_task: true,
    report_turn_started: true,
    report_turn_finished: true,
    interrupt: true,
    stream_output: true,
    heartbeat: false,
    artifact_sources: true,
};

const CLAUDE_CODE_CAPABILITIES: AdapterCapabilities = AdapterCapabilities {
    accept_task: true,
    report_turn_started: false,
    report_turn_finished: true,
    interrupt: false,
    stream_output: false,
    heartbeat: false,
    artifact_sources: false,
};

pub const AGENT_CLIENTS: &[AgentClientSpec] = &[
    AgentClientSpec {
        client_type: "generic",
        capabilities: GENERIC_CAPABILITIES,
        runtime: RuntimeBehavior::InProcessTest,
        dispatch: DispatchBehavior::GenericTestAdapter,
        readiness: ReadinessBehavior::RuntimeManagerImmediate,
        interrupt: InterruptBehavior::Unsupported,
        turn_context: TurnContextBehavior::Disabled,
        adapter_events: AdapterEventBehavior::Disabled,
        startup_hooks: &[],
    },
    AgentClientSpec {
        client_type: "pi",
        capabilities: PI_CAPABILITIES,
        runtime: RuntimeBehavior::Tmux(TmuxRuntimeBehavior {
            command_env: Some("LLMPARTY_PI_TUI_COMMAND"),
            default_command: "pi",
            session_identity_arg: Some("--session-id"),
            hook_log: Some(HookLogBehavior {
                env: "LLMPARTY_PI_HOOK_LOG",
                file_name: "pi-hook.log",
                metadata_key: "pi_hook_log",
            }),
            runtime_config_key: Some("pi"),
        }),
        dispatch: DispatchBehavior::TmuxPaste,
        readiness: ReadinessBehavior::AgentClientEvent,
        interrupt: InterruptBehavior::TmuxInterrupt,
        turn_context: TurnContextBehavior::CurrentTurnFile,
        adapter_events: AdapterEventBehavior::JsonlOutbox {
            file_name: "adapter-events.jsonl",
        },
        startup_hooks: &[],
    },
    AgentClientSpec {
        client_type: "claude_code",
        capabilities: CLAUDE_CODE_CAPABILITIES,
        runtime: RuntimeBehavior::Tmux(TmuxRuntimeBehavior {
            command_env: Some("LLMPARTY_CLAUDE_TUI_COMMAND"),
            default_command: "claude --dangerously-skip-permissions",
            session_identity_arg: None,
            hook_log: Some(HookLogBehavior {
                env: "LLMPARTY_CLAUDE_HOOK_LOG",
                file_name: "claude-hook.log",
                metadata_key: "claude_hook_log",
            }),
            runtime_config_key: Some("claude_code"),
        }),
        dispatch: DispatchBehavior::TmuxPaste,
        readiness: ReadinessBehavior::AgentClientEvent,
        interrupt: InterruptBehavior::Unsupported,
        turn_context: TurnContextBehavior::CurrentTurnFile,
        adapter_events: AdapterEventBehavior::Disabled,
        startup_hooks: &[StartupHook::ClaudeCodeTrustWorkspace],
    },
];

pub fn get_client_spec(client_type: &str) -> Option<&'static AgentClientSpec> {
    if client_type == "generic" && !generic_test_client_enabled() {
        return None;
    }
    AGENT_CLIENTS
        .iter()
        .find(|client| client.client_type == client_type)
}

fn generic_test_client_enabled() -> bool {
    cfg!(test)
        || std::env::current_exe().ok().is_some_and(|path| {
            let path = path.to_string_lossy();
            path.contains("/target/debug/deps/")
                || path.contains("/target/release/deps/")
                || path.contains("\\target\\debug\\deps\\")
                || path.contains("\\target\\release\\deps\\")
        })
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
        assert!(is_supported_client_type("claude_code"));
        assert!(!is_supported_client_type("unsupported"));
    }

    #[test]
    fn generic_client_uses_test_adapter_and_runtime_ready() {
        let generic = get_client_spec("generic").expect("generic spec");
        assert_eq!(generic.runtime, RuntimeBehavior::InProcessTest);
        assert_eq!(generic.dispatch, DispatchBehavior::GenericTestAdapter);
        assert_eq!(
            generic.readiness,
            ReadinessBehavior::RuntimeManagerImmediate
        );
        assert_eq!(generic.interrupt, InterruptBehavior::Unsupported);
        assert_eq!(generic.turn_context, TurnContextBehavior::Disabled);
        assert_eq!(generic.adapter_events, AdapterEventBehavior::Disabled);
    }

    #[test]
    fn tmux_clients_declare_runtime_behavior_in_registry() {
        let pi = get_client_spec("pi").expect("pi spec");
        assert_eq!(
            pi.runtime,
            RuntimeBehavior::Tmux(TmuxRuntimeBehavior {
                command_env: Some("LLMPARTY_PI_TUI_COMMAND"),
                default_command: "pi",
                session_identity_arg: Some("--session-id"),
                runtime_config_key: Some("pi"),
                hook_log: Some(HookLogBehavior {
                    env: "LLMPARTY_PI_HOOK_LOG",
                    file_name: "pi-hook.log",
                    metadata_key: "pi_hook_log",
                }),
            })
        );
        assert_eq!(pi.dispatch, DispatchBehavior::TmuxPaste);
        assert_eq!(pi.readiness, ReadinessBehavior::AgentClientEvent);
        assert_eq!(pi.interrupt, InterruptBehavior::TmuxInterrupt);
        assert_eq!(pi.turn_context, TurnContextBehavior::CurrentTurnFile);
        assert_eq!(
            pi.adapter_events,
            AdapterEventBehavior::JsonlOutbox {
                file_name: "adapter-events.jsonl"
            }
        );

        let claude = get_client_spec("claude_code").expect("claude spec");
        assert_eq!(
            claude.runtime,
            RuntimeBehavior::Tmux(TmuxRuntimeBehavior {
                command_env: Some("LLMPARTY_CLAUDE_TUI_COMMAND"),
                default_command: "claude --dangerously-skip-permissions",
                session_identity_arg: None,
                runtime_config_key: Some("claude_code"),
                hook_log: Some(HookLogBehavior {
                    env: "LLMPARTY_CLAUDE_HOOK_LOG",
                    file_name: "claude-hook.log",
                    metadata_key: "claude_hook_log",
                }),
            })
        );
        assert_eq!(claude.dispatch, DispatchBehavior::TmuxPaste);
        assert_eq!(claude.readiness, ReadinessBehavior::AgentClientEvent);
        assert_eq!(claude.interrupt, InterruptBehavior::Unsupported);
        assert_eq!(claude.turn_context, TurnContextBehavior::CurrentTurnFile);
        assert_eq!(claude.adapter_events, AdapterEventBehavior::Disabled);
    }
}
