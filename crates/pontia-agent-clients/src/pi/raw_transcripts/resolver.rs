use std::fs;

use pontia_core::{Error, Result};

use crate::raw_transcripts::{
    AgentBindingResolveRequest, AgentBindingResolver, ResolvedAgentBinding,
};

#[derive(Debug, Clone, Copy, Default)]
pub struct PiAgentBindingResolver;

impl PiAgentBindingResolver {
    pub const fn new() -> Self {
        Self
    }
}

impl AgentBindingResolver for PiAgentBindingResolver {
    fn client_type(&self) -> &'static str {
        "pi"
    }

    fn resolve(&self, request: &AgentBindingResolveRequest) -> Result<ResolvedAgentBinding> {
        if request.client_type != self.client_type() {
            return Err(Error::CapabilityUnavailable(format!(
                "unsupported binding client_type {} for pi resolver",
                request.client_type
            )));
        }

        let path = request
            .client_session_file
            .as_ref()
            .filter(|path| !path.as_os_str().is_empty())
            .cloned()
            .ok_or_else(|| {
                Error::CapabilityUnavailable(
                    "source_unavailable: pi agent binding has no client_session_file".to_string(),
                )
            })?;
        fs::metadata(&path).map_err(|error| {
            Error::CapabilityUnavailable(format!(
                "source_unavailable: pi session file {} is unavailable: {error}",
                path.display()
            ))
        })?;

        Ok(ResolvedAgentBinding {
            id: request.id.clone(),
            client_type: request.client_type.clone(),
            format: "pi-jsonl".to_string(),
            path,
            fingerprint: None,
        })
    }
}
