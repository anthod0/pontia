mod claude_code;
mod generic;
mod pi;
mod types;

pub use types::*;

pub const AGENT_CLIENTS: &[AgentClientDefinition] =
    &[generic::DEFINITION, pi::DEFINITION, claude_code::DEFINITION];

pub fn get_client_definition(client_type: &str) -> Option<&'static AgentClientDefinition> {
    if client_type == "generic" && !generic_test_client_enabled() {
        return None;
    }
    AGENT_CLIENTS
        .iter()
        .find(|client| client.client_type == client_type)
}

pub fn get_client_spec(client_type: &str) -> Option<&'static AgentClientDefinition> {
    get_client_definition(client_type)
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
    get_client_definition(client_type).is_some()
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
        let generic = get_client_definition("generic").expect("generic definition");
        assert_eq!(generic.backend.runtime, RuntimeBehavior::InProcessTest);
        assert_eq!(
            generic.backend.dispatch,
            DispatchBehavior::GenericTestAdapter
        );
        assert_eq!(
            generic.backend.readiness,
            ReadinessBehavior::RuntimeManagerImmediate
        );
        assert_eq!(generic.backend.interrupt, InterruptBehavior::Unsupported);
        assert_eq!(generic.backend.terminate, TerminateBehavior::RuntimeManager);
        assert_eq!(generic.backend.turn_context, TurnContextBehavior::Disabled);
        assert_eq!(
            generic.backend.adapter_events,
            AdapterEventBehavior::Disabled
        );
        assert!(!generic.capabilities.timeline);
        assert_eq!(generic.backend.transcript, TranscriptBehavior::Unsupported);
        assert_eq!(
            generic.backend.client_session_identity,
            ClientSessionIdentityBehavior::Unsupported
        );
    }

    #[test]
    fn client_session_identity_requirement_is_declared_by_client_definition() {
        let pi = get_client_definition("pi").expect("pi definition");
        assert_eq!(
            pi.backend.client_session_identity,
            ClientSessionIdentityBehavior::RequiredOnReady
        );

        let claude = get_client_definition("claude_code").expect("claude definition");
        assert_eq!(
            claude.backend.client_session_identity,
            ClientSessionIdentityBehavior::OptionalOnReady
        );
    }

    #[test]
    fn tmux_clients_declare_runtime_behavior_in_registry() {
        let pi = get_client_definition("pi").expect("pi definition");
        assert_eq!(
            pi.backend.runtime,
            RuntimeBehavior::Tmux(TmuxRuntimeBehavior {
                command_env: Some("PONTIA_PI_TUI_COMMAND"),
                default_command: "pi --approve",
                session_identity_arg: Some("--session-id"),
                runtime_config_key: Some("pi"),
                hook_log: Some(HookLogBehavior {
                    env: "PONTIA_PI_HOOK_LOG",
                    file_name: "pi-hook.log",
                    metadata_key: "pi_hook_log",
                }),
            })
        );
        assert_eq!(pi.backend.dispatch, DispatchBehavior::TmuxPaste);
        assert_eq!(pi.backend.readiness, ReadinessBehavior::AgentClientEvent);
        assert_eq!(pi.backend.interrupt, InterruptBehavior::TmuxInterrupt);
        assert_eq!(
            pi.backend.terminate,
            TerminateBehavior::TmuxSendKeys(&["C-c", "C-c"])
        );
        assert_eq!(
            pi.backend.turn_context,
            TurnContextBehavior::CurrentTurnFile
        );
        assert_eq!(
            pi.backend.system_prompt_injection,
            SystemPromptInjectionBehavior::AppendFromExternalApi
        );
        assert!(pi.capabilities.timeline);
        assert_eq!(pi.backend.transcript, TranscriptBehavior::PiJsonl);
        assert_eq!(
            pi.backend.adapter_events,
            AdapterEventBehavior::JsonlOutbox {
                file_name: "adapter-events.jsonl"
            }
        );

        let claude = get_client_definition("claude_code").expect("claude definition");
        assert_eq!(
            claude.backend.runtime,
            RuntimeBehavior::Tmux(TmuxRuntimeBehavior {
                command_env: Some("PONTIA_CLAUDE_TUI_COMMAND"),
                default_command: "claude --dangerously-skip-permissions",
                session_identity_arg: None,
                runtime_config_key: Some("claude_code"),
                hook_log: Some(HookLogBehavior {
                    env: "PONTIA_CLAUDE_HOOK_LOG",
                    file_name: "claude-hook.log",
                    metadata_key: "claude_hook_log",
                }),
            })
        );
        assert_eq!(claude.backend.dispatch, DispatchBehavior::TmuxPaste);
        assert_eq!(
            claude.backend.readiness,
            ReadinessBehavior::AgentClientEvent
        );
        assert_eq!(claude.backend.interrupt, InterruptBehavior::Unsupported);
        assert_eq!(
            claude.backend.terminate,
            TerminateBehavior::TmuxSendKeys(&["C-c"])
        );
        assert_eq!(
            claude.backend.turn_context,
            TurnContextBehavior::CurrentTurnFile
        );
        assert_eq!(
            claude.backend.system_prompt_injection,
            SystemPromptInjectionBehavior::Disabled
        );
        assert!(!claude.capabilities.timeline);
        assert_eq!(claude.backend.transcript, TranscriptBehavior::Unsupported);
        assert_eq!(
            claude.backend.adapter_events,
            AdapterEventBehavior::Disabled
        );
    }
}
