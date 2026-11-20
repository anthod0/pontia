use super::CodexService;
use crate::instructions::CodexInstructions;
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

    pub(super) fn instructions(&self) -> CodexInstructions {
        CodexInstructions::new(self.pool.clone())
    }

    pub(super) async fn resume_params(
        &self,
        session: &str,
        thread: &str,
        connection: &crate::runtime::protocol::Connection,
    ) -> Result<Value> {
        self.instructions()
            .configured_codex_binding(session)
            .await?;
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
    let mut instructions = match operation {
        ThreadOperation::Start => {
            let resolved = pontia_agent_control::SessionInstructionsService::new(state.db())
                .with_clients(state.clients())
                .resolve("codex", None, true)?;
            resolved
                .map(|resolved| {
                    serde_json::from_value::<crate::instructions::CodexInstructionsBinding>(
                        resolved,
                    )
                })
                .transpose()?
        }
        ThreadOperation::Resume => match params["threadId"].as_str() {
            Some(thread) => {
                CodexInstructions::new(state.db())
                    .instructions_for_thread(thread)
                    .await?
            }
            None => None,
        },
        ThreadOperation::Fork => unreachable!(),
    };
    let Some(instructions) = &mut instructions else {
        return Ok(None);
    };
    let existing = instructions
        .native_developer_instructions
        .as_deref()
        .or_else(|| params["developerInstructions"].as_str())
        .or_else(|| params["config"]["developer_instructions"].as_str())
        .or_else(|| config["developer_instructions"].as_str());
    let existing = existing.map(str::to_owned);
    if operation == ThreadOperation::Start {
        instructions.native_developer_instructions = existing.clone();
    }
    if let Some(prompt) = &instructions.system_prompt {
        params["developerInstructions"] = json!(append_instructions(existing.as_deref(), prompt));
    }
    if operation == ThreadOperation::Start {
        Ok(Some(serde_json::to_value(instructions)?))
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
