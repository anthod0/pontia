use super::CodexService;
use pontia_application::{AgentProfileService, CodexProfileBinding};
use pontia_core::{Error, Result};
use serde_json::{Value, json};

impl CodexService {
    pub(super) fn profiles(&self) -> AgentProfileService {
        AgentProfileService::new(self.pool.clone())
    }

    pub(super) async fn resume_params(&self, session: &str, thread: &str) -> Result<Value> {
        let profile = self.profiles().configured_codex_binding(session).await?;
        let mut params = json!({"threadId":thread,"excludeTurns":true});
        apply_profile(&mut params, profile.as_ref())?;
        Ok(params)
    }
}

/// A TUI may resume any thread, including a legacy Profile binding. Resolve the
/// target identity for each request rather than inheriting the TUI owner's Profile.
pub(crate) async fn guard_tui_request(
    profiles: &AgentProfileService,
    request: &mut Value,
) -> Result<()> {
    if !matches!(
        request["method"].as_str(),
        Some("thread/resume" | "turn/start" | "turn/steer")
    ) {
        return Ok(());
    }
    let Some(thread) = request["params"]["threadId"].as_str() else {
        return Ok(());
    };
    let profile = profiles.codex_profile_for_thread(thread).await?;
    if request["method"] == "thread/resume" {
        apply_profile(&mut request["params"], profile.as_ref())?;
    }
    Ok(())
}

pub(super) fn apply_profile(
    params: &mut Value,
    profile: Option<&CodexProfileBinding>,
) -> Result<()> {
    if let Some(prompt) = profile.and_then(|profile| profile.system_prompt.as_deref()) {
        for existing in [
            &params["developerInstructions"],
            &params["config"]["developer_instructions"],
        ] {
            if !existing.is_null() && existing.as_str() != Some(prompt) {
                return Err(Error::StateConflict("Codex Profile instructions are fixed for this Session; conflicting developer instructions are unsupported".into()));
            }
        }
        params["developerInstructions"] = json!(prompt);
    }
    Ok(())
}
