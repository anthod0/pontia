use super::CodexService;
use crate::instructions::CodexInstructions;
use pontia_core::Result;
use serde_json::{Value, json};

impl CodexService {
    pub(super) async fn apply_session_mode(&self, session: &str, params: &mut Value) -> Result<()> {
        if self
            .store
            .queries()
            .session_mode(session)
            .await?
            .bypasses_approvals()
        {
            params["approvalPolicy"] = json!("never");
            params["sandbox"] = json!("danger-full-access");
        }
        Ok(())
    }

    pub(super) fn instructions(&self) -> CodexInstructions {
        CodexInstructions::new(self.store.clone())
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
    state: &pontia_agent_control::ClientServiceContext,
    operation: crate::proxy::ThreadOperation,
    params: &mut Value,
    config: &Value,
) -> Result<Option<Value>> {
    use crate::proxy::ThreadOperation;
    let mut instructions = match operation {
        ThreadOperation::Start => {
            let resolved = state
                .store()
                .instructions()
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
                CodexInstructions::new(state.store())
                    .instructions_for_thread(thread)
                    .await?
            }
            None => None,
        },
        ThreadOperation::Fork => return Ok(None),
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
