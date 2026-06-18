mod generic_test;
pub mod pi;
pub mod raw_transcripts;
mod types;

pub use generic_test::GenericTestClient;
pub use types::*;

use raw_transcripts::{AgentBindingResolver, RawTranscriptParser};

pub const AGENT_CLIENTS: &[AgentClientSpec] = &[generic_test::SPEC, pi::SPEC];

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
        assert!(!is_supported_client_type("unsupported"));
    }
}
