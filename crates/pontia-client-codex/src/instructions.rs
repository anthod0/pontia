use pontia_agent_control::ClientControlStore;
use pontia_agent_control::client_contract::{ClientInstructions, InstructionsBindingInput};

pub struct CodexInstructions {
    store: ClientControlStore,
}
impl CodexInstructions {
    pub fn new(store: ClientControlStore) -> Self {
        Self { store }
    }
}

pub(crate) struct CodexInstructionsPolicy;
impl ClientInstructions for CodexInstructionsPolicy {
    fn inject_standalone_session_instructions(&self) -> bool {
        true
    }
    fn bind(&self, input: &InstructionsBindingInput<'_>) -> Result<serde_json::Value> {
        Ok(serde_json::to_value(CodexInstructionsBinding {
            contract_version: 1,
            system_prompt: input.system_prompt.map(str::to_owned),
            native_developer_instructions: None,
        })?)
    }
}
use pontia_core::{Error, Result};
use serde::{Deserialize, Serialize};

/// Immutable execution content, committed with the Session creation event.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CodexInstructionsBinding {
    pub contract_version: u32,
    pub system_prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_developer_instructions: Option<String>,
}

impl CodexInstructions {
    /// Confirms that the adapter-bound native thread carries the fixed creation-time instructions.
    pub async fn confirm_codex_configuration(&self, session: &str, native_key: &str) -> Result<()> {
        if self.codex_binding(session).await?.is_none() {
            return Err(unverified());
        }
        let binding = self.store.bindings().binding_for_session(session).await?;
        match binding {
            Some(binding)
                if binding.client_type == "codex" && binding.client_session_key == native_key =>
            {
                Ok(())
            }
            _ => Err(unverified()),
        }
    }

    pub async fn instructions_for_thread(
        &self,
        native_key: &str,
    ) -> Result<Option<CodexInstructionsBinding>> {
        let Some(binding) = self
            .store
            .bindings()
            .binding_for_client_session("codex", native_key)
            .await?
        else {
            return Ok(None);
        };
        self.configured_codex_binding(&binding.session_id).await
    }

    /// Reads the creation-time snapshot, never the current mutable configuration.
    pub async fn codex_binding(&self, session: &str) -> Result<Option<CodexInstructionsBinding>> {
        let Some(snapshot) = self
            .store
            .instructions()
            .session_instructions(session)
            .await?
        else {
            return Ok(None);
        };
        let snapshot: CodexInstructionsBinding =
            serde_json::from_value(snapshot).map_err(|_| unverified())?;
        if snapshot.contract_version != 1 {
            return Err(unverified());
        }
        Ok(Some(snapshot))
    }

    pub async fn configured_codex_binding(
        &self,
        session: &str,
    ) -> Result<Option<CodexInstructionsBinding>> {
        let instructions = self.codex_binding(session).await?;
        let Some(instructions) = instructions else {
            return Ok(None);
        };
        let binding = self.store.bindings().binding_for_session(session).await?;
        match binding {
            Some(binding) if binding.client_type == "codex" => Ok(Some(instructions)),
            _ => Err(unverified()),
        }
    }
}

fn unverified() -> Error {
    Error::StateConflict("Codex instruction binding is unverified".into())
}
