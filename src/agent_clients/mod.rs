mod claude_code;
mod generic;
mod pi;
mod types;

pub use types::*;

pub const AGENT_CLIENTS: &[AgentClientSpec] = &[generic::SPEC, pi::SPEC, claude_code::SPEC];

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
                command_env: Some("PILOTFY_PI_TUI_COMMAND"),
                default_command: "pi",
                session_identity_arg: Some("--session-id"),
                runtime_config_key: Some("pi"),
                hook_log: Some(HookLogBehavior {
                    env: "PILOTFY_PI_HOOK_LOG",
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
            pi.system_prompt_injection,
            SystemPromptInjectionBehavior::AppendFromExternalApi
        );
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
                command_env: Some("PILOTFY_CLAUDE_TUI_COMMAND"),
                default_command: "claude --dangerously-skip-permissions",
                session_identity_arg: None,
                runtime_config_key: Some("claude_code"),
                hook_log: Some(HookLogBehavior {
                    env: "PILOTFY_CLAUDE_HOOK_LOG",
                    file_name: "claude-hook.log",
                    metadata_key: "claude_hook_log",
                }),
            })
        );
        assert_eq!(claude.dispatch, DispatchBehavior::TmuxPaste);
        assert_eq!(claude.readiness, ReadinessBehavior::AgentClientEvent);
        assert_eq!(claude.interrupt, InterruptBehavior::Unsupported);
        assert_eq!(claude.turn_context, TurnContextBehavior::CurrentTurnFile);
        assert_eq!(
            claude.system_prompt_injection,
            SystemPromptInjectionBehavior::Disabled
        );
        assert_eq!(claude.adapter_events, AdapterEventBehavior::Disabled);
    }
}
