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
    pub session_identity_arg: Option<&'static str>,
    pub hook_log_env: Option<&'static str>,
    pub runtime_config_key: Option<&'static str>,
    pub dispatch_mode: DispatchMode,
    pub readiness_mode: ReadinessMode,
    pub startup_hooks: &'static [StartupHook],
    pub adapter_event_outbox: bool,
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
        session_identity_arg: None,
        hook_log_env: None,
        runtime_config_key: None,
        dispatch_mode: DispatchMode::GenericTestAdapter,
        readiness_mode: ReadinessMode::RuntimeManagerImmediate,
        startup_hooks: &[],
        adapter_event_outbox: false,
    },
    AgentClientSpec {
        client_type: "pi",
        capabilities: PI_CAPABILITIES,
        command_env: Some("LLMPARTY_PI_TUI_COMMAND"),
        default_command: Some("pi"),
        session_identity_arg: Some("--session-id"),
        hook_log_env: Some("LLMPARTY_PI_HOOK_LOG"),
        runtime_config_key: Some("pi"),
        dispatch_mode: DispatchMode::TmuxPaste,
        readiness_mode: ReadinessMode::AgentClientEvent,
        startup_hooks: &[],
        adapter_event_outbox: true,
    },
    AgentClientSpec {
        client_type: "claude_code",
        capabilities: CLAUDE_CODE_CAPABILITIES,
        command_env: Some("LLMPARTY_CLAUDE_TUI_COMMAND"),
        default_command: Some("claude --dangerously-skip-permissions"),
        session_identity_arg: None,
        hook_log_env: Some("LLMPARTY_CLAUDE_HOOK_LOG"),
        runtime_config_key: Some("claude_code"),
        dispatch_mode: DispatchMode::TmuxPaste,
        readiness_mode: ReadinessMode::AgentClientEvent,
        startup_hooks: &[StartupHook::ClaudeCodeTrustWorkspace],
        adapter_event_outbox: false,
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
    fn pi_and_claude_are_tmux_clients_that_wait_for_agent_ready() {
        let pi = get_client_spec("pi").expect("pi spec");
        assert_eq!(pi.dispatch_mode, DispatchMode::TmuxPaste);
        assert_eq!(pi.readiness_mode, ReadinessMode::AgentClientEvent);
        assert_eq!(pi.command_env, Some("LLMPARTY_PI_TUI_COMMAND"));
        assert_eq!(pi.default_command, Some("pi"));
        assert_eq!(pi.session_identity_arg, Some("--session-id"));
        assert_eq!(pi.hook_log_env, Some("LLMPARTY_PI_HOOK_LOG"));
        assert_eq!(pi.runtime_config_key, Some("pi"));
        assert!(pi.adapter_event_outbox);

        let claude = get_client_spec("claude_code").expect("claude spec");
        assert_eq!(claude.dispatch_mode, DispatchMode::TmuxPaste);
        assert_eq!(claude.readiness_mode, ReadinessMode::AgentClientEvent);
        assert_eq!(claude.command_env, Some("LLMPARTY_CLAUDE_TUI_COMMAND"));
        assert_eq!(
            claude.default_command,
            Some("claude --dangerously-skip-permissions")
        );
        assert_eq!(claude.session_identity_arg, None);
        assert_eq!(claude.hook_log_env, Some("LLMPARTY_CLAUDE_HOOK_LOG"));
        assert_eq!(claude.runtime_config_key, Some("claude_code"));
        assert!(!claude.adapter_event_outbox);
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
        assert_eq!(generic.session_identity_arg, None);
        assert_eq!(generic.hook_log_env, None);
        assert_eq!(generic.runtime_config_key, None);
        assert!(!generic.adapter_event_outbox);
    }
}
