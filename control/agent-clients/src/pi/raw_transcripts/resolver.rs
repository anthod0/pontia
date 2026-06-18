use std::{fs, path::PathBuf};

use pontia_core::{Error, Result};

use crate::raw_transcripts::{
    AgentBindingResolveRequest, AgentBindingResolver, ResolvedAgentBinding,
};

#[derive(Debug, Clone)]
pub struct PiAgentBindingResolver {
    agent_dir: PathBuf,
}

impl PiAgentBindingResolver {
    pub fn new() -> Self {
        Self {
            agent_dir: default_pi_agent_dir(),
        }
    }

    pub fn with_agent_dir(agent_dir: PathBuf) -> Self {
        Self { agent_dir }
    }
}

impl Default for PiAgentBindingResolver {
    fn default() -> Self {
        Self::new()
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

        let session_dir = pi_session_dir(&self.agent_dir, &request.launch_cwd);
        let suffix = format!("_{}.jsonl", request.client_session_key);
        let mut matches = Vec::new();
        let entries = fs::read_dir(&session_dir).map_err(|err| {
            Error::CapabilityUnavailable(format!(
                "source_unavailable: pi session dir {} is unavailable: {err}",
                session_dir.display()
            ))
        })?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(&suffix))
            {
                matches.push(path);
            }
        }
        matches.sort();

        let Some(path) = matches.pop() else {
            return Err(Error::CapabilityUnavailable(format!(
                "source_unavailable: pi session file for key {} not found under {}",
                request.client_session_key,
                session_dir.display()
            )));
        };

        Ok(ResolvedAgentBinding {
            id: request.id.clone(),
            client_type: request.client_type.clone(),
            format: "pi-jsonl".to_string(),
            path,
            fingerprint: None,
        })
    }
}

fn default_pi_agent_dir() -> PathBuf {
    if let Ok(path) = std::env::var("PI_AGENT_DIR") {
        return PathBuf::from(path);
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".pi").join("agent")
}

fn pi_session_dir(agent_dir: &std::path::Path, cwd: &std::path::Path) -> PathBuf {
    let resolved = cwd.to_string_lossy();
    let safe_path = resolved
        .trim_start_matches(['/', '\\'])
        .replace(['/', '\\', ':'], "-");
    agent_dir.join("sessions").join(format!("--{safe_path}--"))
}
