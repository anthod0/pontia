use crate::adapters::AdapterCapabilities;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatchMode {
    GenericTestAdapter,
    TmuxPaste,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadinessMode {
    RuntimeManagerImmediate,
    AgentClientEvent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupHook {
    ClaudeCodeTrustWorkspace,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentClientSpec {
    pub client_type: &'static str,
    pub capabilities: AdapterCapabilities,
    pub command_env: Option<&'static str>,
    pub default_command: Option<&'static str>,
    pub dispatch_mode: DispatchMode,
    pub readiness_mode: ReadinessMode,
    pub startup_hooks: &'static [StartupHook],
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
        command_env: None,
        default_command: None,
        dispatch_mode: DispatchMode::GenericTestAdapter,
        readiness_mode: ReadinessMode::RuntimeManagerImmediate,
        startup_hooks: &[],
    },
    AgentClientSpec {
        client_type: "pi",
        capabilities: PI_CAPABILITIES,
        command_env: Some("LLMPARTY_PI_TUI_COMMAND"),
        default_command: Some("pi"),
        dispatch_mode: DispatchMode::TmuxPaste,
        readiness_mode: ReadinessMode::AgentClientEvent,
        startup_hooks: &[],
    },
    AgentClientSpec {
        client_type: "claude_code",
        capabilities: CLAUDE_CODE_CAPABILITIES,
        command_env: Some("LLMPARTY_CLAUDE_TUI_COMMAND"),
        default_command: Some("claude --dangerously-skip-permissions"),
        dispatch_mode: DispatchMode::TmuxPaste,
        readiness_mode: ReadinessMode::AgentClientEvent,
        startup_hooks: &[StartupHook::ClaudeCodeTrustWorkspace],
    },
];

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
        assert!(is_supported_client_type("claude_code"));
        assert!(!is_supported_client_type("unsupported"));
    }

    #[test]
    fn pi_and_claude_are_tmux_clients_that_wait_for_agent_ready() {
        let pi = get_client_spec("pi").expect("pi spec");
        assert_eq!(pi.dispatch_mode, DispatchMode::TmuxPaste);
        assert_eq!(pi.readiness_mode, ReadinessMode::AgentClientEvent);
        assert_eq!(pi.command_env, Some("LLMPARTY_PI_TUI_COMMAND"));
        assert_eq!(pi.default_command, Some("pi"));

        let claude = get_client_spec("claude_code").expect("claude spec");
        assert_eq!(claude.dispatch_mode, DispatchMode::TmuxPaste);
        assert_eq!(claude.readiness_mode, ReadinessMode::AgentClientEvent);
        assert_eq!(claude.command_env, Some("LLMPARTY_CLAUDE_TUI_COMMAND"));
        assert_eq!(
            claude.default_command,
            Some("claude --dangerously-skip-permissions")
        );
        assert_eq!(
            claude.startup_hooks,
            &[StartupHook::ClaudeCodeTrustWorkspace]
        );
    }

    #[test]
    fn generic_client_uses_test_adapter_and_runtime_ready() {
        let generic = get_client_spec("generic").expect("generic spec");
        assert_eq!(generic.dispatch_mode, DispatchMode::GenericTestAdapter);
        assert_eq!(
            generic.readiness_mode,
            ReadinessMode::RuntimeManagerImmediate
        );
        assert_eq!(generic.command_env, None);
        assert_eq!(generic.default_command, None);
    }
}
