use super::CodexService;
use crate::profiles::CodexProfiles;
use pontia_core::Result;
use serde_json::{Value, json};

impl CodexService {
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
) -> Result<Option<Value>> {
    use crate::proxy::ThreadOperation;
    if params["ephemeral"] == true || operation == ThreadOperation::Fork {
        return Ok(None);
    }
    let mut profile = match operation {
        ThreadOperation::Start => {
            let resolved = pontia_agent_control::AgentProfileService::new(state.db())
                .with_clients(state.clients())
                .resolve_for_client("codex", None, None)
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
    let runtime = crate::runtime::CodexRuntime::ensure(state.pontia_home()).await?;
    let connection = runtime.connection().await?;
    let config = connection
        .call(
            "config/read",
            json!({"cwd":params["cwd"],"includeLayers":false}),
        )
        .await?;
    let existing = profile
        .native_developer_instructions
        .as_deref()
        .or_else(|| params["developerInstructions"].as_str())
        .or_else(|| params["config"]["developer_instructions"].as_str())
        .or_else(|| config["config"]["developer_instructions"].as_str());
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
