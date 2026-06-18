mod generic_test;
mod pi;
mod types;

pub use generic_test::GenericTestClient;
pub use types::*;

pub const AGENT_CLIENTS: &[AgentClientSpec] = &[generic_test::SPEC, pi::SPEC];

pub fn run_startup_hooks(
    hooks: &[StartupHook],
    _workspace: &std::path::Path,
) -> crate::error::Result<()> {
    match hooks {
        [] => Ok(()),
        [hook, ..] => match *hook {},
    }
}

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
        assert!(!is_supported_client_type("unsupported"));
    }

    #[test]
    fn generic_client_uses_test_client_and_runtime_ready() {
        let generic = get_client_spec("generic").expect("generic spec");
        assert_eq!(generic.adapter.runtime, RuntimeBehavior::InProcessTest);
        assert_eq!(
            generic.adapter.dispatch,
            DispatchBehavior::GenericTestClient
        );
        assert_eq!(
            generic.adapter.readiness,
            ReadinessBehavior::RuntimeManagerImmediate
        );
        assert_eq!(generic.adapter.interrupt, InterruptBehavior::Unsupported);
        assert_eq!(generic.adapter.terminate, TerminateBehavior::RuntimeManager);
        assert_eq!(generic.adapter.turn_context, TurnContextBehavior::Disabled);
        assert_eq!(
            generic.adapter.adapter_events,
            AdapterEventBehavior::Disabled
        );
        assert!(!generic.capabilities.timeline);
        assert_eq!(generic.adapter.transcript, TranscriptBehavior::Unsupported);
        assert_eq!(
            generic.adapter.client_session_identity,
            ClientSessionIdentityBehavior::Unsupported
        );
    }

    #[test]
    fn client_session_identity_requirement_is_declared_by_client_spec() {
        let pi = get_client_spec("pi").expect("pi spec");
        assert_eq!(
            pi.adapter.client_session_identity,
            ClientSessionIdentityBehavior::RequiredOnReady
        );
    }

    #[test]
    fn tmux_clients_declare_runtime_behavior_in_registry() {
        let pi = get_client_spec("pi").expect("pi spec");
        assert_eq!(
            pi.adapter.runtime,
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
        assert_eq!(pi.adapter.dispatch, DispatchBehavior::TmuxPaste);
        assert_eq!(pi.adapter.readiness, ReadinessBehavior::AgentClientEvent);
        assert_eq!(pi.adapter.interrupt, InterruptBehavior::TmuxInterrupt);
        assert_eq!(
            pi.adapter.terminate,
            TerminateBehavior::TmuxSendKeys(&["C-c", "C-c"])
        );
        assert_eq!(
            pi.adapter.turn_context,
            TurnContextBehavior::CurrentTurnFile
        );
        assert_eq!(
            pi.adapter.system_prompt_injection,
            SystemPromptInjectionBehavior::AppendFromExternalApi
        );
        assert!(pi.capabilities.timeline);
        assert_eq!(pi.adapter.transcript, TranscriptBehavior::PiJsonl);
        assert_eq!(
            pi.adapter.adapter_events,
            AdapterEventBehavior::JsonlOutbox {
                file_name: "adapter-events.jsonl"
            }
        );
    }
}
