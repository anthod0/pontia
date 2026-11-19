use super::CodexService;
use crate::profiles::CodexProfiles;
use pontia_core::Result;
use serde_json::{Value, json};

impl CodexService {
    pub(super) async fn apply_execution_policy(
        &self,
        session: &str,
        params: &mut Value,
    ) -> Result<()> {
        if pontia_agent_control::ControlQueryService::new(self.pool.clone())
            .session_execution_policy(session)
            .await?
            .yolo
        {
            params["approvalPolicy"] = json!("never");
            params["sandbox"] = json!("danger-full-access");
        }
        Ok(())
    }

    pub(super) fn profiles(&self) -> CodexProfiles {
        CodexProfiles::new(self.pool.clone())
    }

    pub(super) async fn resume_params(
        &self,
        session: &str,
        thread: &str,
        connection: &crate::runtime::protocol::Connection,
    ) -> Result<Value> {
        self.profiles().configured_codex_binding(session).await?;
        let binding = self.binding(session).await?;
        let config = self
            .session_config(session, connection, &binding.launch_cwd)
            .await?;
        let params = json!({"threadId":thread,"excludeTurns":true,"config":config});
        Ok(params)
    }
}

pub(crate) async fn prepare_proxy_request(
    state: &pontia_agent_control::AgentControl,
    operation: crate::proxy::ThreadOperation,
    params: &mut Value,
    config: &Value,
) -> Result<Option<Value>> {
    use crate::proxy::ThreadOperation;
    if params["ephemeral"] == true || operation == ThreadOperation::Fork {
        return Ok(None);
    }
    if operation == ThreadOperation::Resume
        && let Some(thread) = params["threadId"].as_str()
        && let Some(binding) = pontia_agent_control::AgentBindingService::new(state.db())
            .binding_for_client_session("codex", thread)
            .await?
        && state
            .queries()
            .session_execution_policy(&binding.session_id)
            .await?
            .yolo
    {
        params["approvalPolicy"] = json!("never");
        params["sandbox"] = json!("danger-full-access");
    }
    let mut profile = match operation {
        ThreadOperation::Start => {
            let resolved = pontia_agent_control::AgentProfileService::new(state.db())
                .with_clients(state.clients())
                .resolve_for_client("codex", None, None, None)
                .await?;
            resolved
                .map(|resolved| {
                    serde_json::from_value::<crate::profiles::CodexProfileBinding>(resolved.binding)
                })
                .transpose()?
        }
        ThreadOperation::Resume => match params["threadId"].as_str() {
            Some(thread) => {
                CodexProfiles::new(state.db())
                    .codex_profile_for_thread(thread)
                    .await?
            }
            None => None,
        },
        ThreadOperation::Fork => unreachable!(),
    };
    let Some(profile) = &mut profile else {
        return Ok(None);
    };
    let existing = profile
        .native_developer_instructions
        .as_deref()
        .or_else(|| params["developerInstructions"].as_str())
        .or_else(|| params["config"]["developer_instructions"].as_str())
        .or_else(|| config["developer_instructions"].as_str());
    let existing = existing.map(str::to_owned);
    if operation == ThreadOperation::Start {
        profile.native_developer_instructions = existing.clone();
    }
    if let Some(prompt) = &profile.system_prompt {
        params["developerInstructions"] = json!(append_instructions(existing.as_deref(), prompt));
    }
    if operation == ThreadOperation::Start {
        Ok(Some(serde_json::to_value(profile)?))
    } else {
        Ok(None)
    }
}

pub(super) fn append_instructions(existing: Option<&str>, prompt: &str) -> String {
    match existing {
        Some(existing) if existing == prompt || existing.ends_with(&format!("\n\n{prompt}")) => {
            existing.into()
        }
        Some(existing) if !existing.is_empty() => format!("{existing}\n\n{prompt}"),
        _ => prompt.into(),
    }
}
