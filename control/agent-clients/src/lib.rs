pub mod claude;
mod generic_test;
pub mod pi;
pub mod raw_transcripts;
mod types;

pub use generic_test::{GenericTestClient, InProcessRecordedDispatchBehavior};
pub use types::*;

use raw_transcripts::{AgentBindingResolver, RawTranscriptParser};

pub const AGENT_CLIENTS: &[AgentClientSpec] = &[generic_test::SPEC, pi::SPEC, claude::SPEC];

pub fn default_real_client_spec() -> &'static AgentClientSpec {
    &pi::SPEC
}

pub fn default_real_client_type() -> &'static str {
    default_real_client_spec().client_type
}

pub fn client_session_identity_required_on_ready(client_type: &str) -> bool {
    get_client_spec(client_type).is_some_and(|spec| {
        spec.adapter.client_session_identity == ClientSessionIdentityBehavior::RequiredOnReady
    })
}

pub struct RawTranscriptBackend {
    pub resolver: Box<dyn AgentBindingResolver + Send + Sync>,
    pub parser: Box<dyn RawTranscriptParser + Send + Sync>,
}

pub fn raw_transcript_backend_for(client_type: &str) -> Option<RawTranscriptBackend> {
    let spec = get_client_spec(client_type)?;
    match spec.adapter.transcript {
        TranscriptBehavior::Unsupported => None,
        TranscriptBehavior::PiJsonl => Some(RawTranscriptBackend {
            resolver: Box::new(pi::raw_transcripts::PiAgentBindingResolver::new()),
            parser: Box::new(pi::raw_transcripts::PiJsonlParser::new()),
        }),
    }
}

pub fn in_process_capabilities(client_type: &str) -> Option<AgentClientCapabilities> {
    match client_type {
        "generic" => Some(GenericTestClient.capabilities()),
        _ => None,
    }
}

pub fn in_process_recorded_dispatch_behavior(
    client_type: &str,
) -> Option<InProcessRecordedDispatchBehavior> {
    match client_type {
        "generic" => Some(GenericTestClient::behavior()),
        _ => None,
    }
}

pub fn accept_in_process_input(client_type: &str, input: AgentInput) -> pontia_core::Result<()> {
    match client_type {
        "generic" => GenericTestClient.accept_input(input),
        _ => Err(pontia_core::error::Error::Domain(format!(
            "{client_type} does not support in-process input dispatch"
        ))),
    }
}

pub fn run_startup_hooks(
    hooks: &[StartupHook],
    _workspace: &std::path::Path,
) -> pontia_core::Result<()> {
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
        assert!(is_supported_client_type("claude"));
        assert!(!is_supported_client_type("unsupported"));
    }

    #[test]
    fn claude_spec_matches_phase_2_contract() {
        let spec = get_client_spec("claude").expect("claude client spec registered");
        assert_eq!(spec.client_type, "claude");
        assert_eq!(
            spec.capabilities,
            AgentClientCapabilities {
                accept_task: true,
                report_turn_started: true,
                report_turn_finished: true,
                interrupt: true,
                stream_output: false,
                heartbeat: false,
                timeline: false,
                context_usage: ContextUsageCapability::Unsupported,
            }
        );
        assert_eq!(spec.adapter.dispatch, DispatchBehavior::TmuxPaste);
        assert_eq!(spec.adapter.readiness, ReadinessBehavior::AgentClientEvent);
        assert_eq!(
            spec.adapter.client_session_identity,
            ClientSessionIdentityBehavior::RequiredOnReady
        );
        assert_eq!(
            spec.adapter.turn_context,
            TurnContextBehavior::InternalApiClaim
        );
        assert_eq!(spec.adapter.current_turn_id, CurrentTurnIdBehavior::Omit);
        assert_eq!(
            spec.adapter.turn_lifecycle,
            TurnLifecycleBehavior::ClientManagedForInteractiveTmux
        );
        assert_eq!(spec.runtime_binding_kind(), Some("claude_tui"));
        assert_eq!(
            spec.adapter.system_prompt_injection,
            SystemPromptInjectionBehavior::Disabled
        );
        assert_eq!(spec.adapter.transcript, TranscriptBehavior::Unsupported);

        let runtime = spec.tmux_runtime().expect("claude uses tmux runtime");
        assert_eq!(runtime.command_env, Some("PONTIA_CLAUDE_TUI_COMMAND"));
        assert_eq!(runtime.default_command, "claude");
        assert_eq!(runtime.startup_args, &[] as &[&str]);
        assert_eq!(runtime.session_identity_arg, None);
        assert_eq!(runtime.runtime_config_key, Some("claude"));
        let hook_log = runtime.hook_log.expect("claude hook log configured");
        assert_eq!(hook_log.file_name, "claude-hook.log");
        assert_eq!(hook_log.metadata_key, "claude_hook_log");
    }

    #[test]
    fn claude_raw_transcript_backend_is_unsupported_in_phase_2() {
        assert!(raw_transcript_backend_for("claude").is_none());
    }
}
