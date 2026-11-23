use agent_client_protocol::v1::schema::Meta;
use pontia_agent_control::client_contract::{ClientInstructions, InstructionsBindingInput};
use pontia_core::Result;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub(crate) struct InstructionsPolicy;

impl ClientInstructions for InstructionsPolicy {
    fn inject_standalone_session_instructions(&self) -> bool {
        true
    }

    fn bind(&self, input: &InstructionsBindingInput<'_>) -> Result<Value> {
        Ok(serde_json::to_value(Instructions {
            system_prompt: input.system_prompt.map(str::to_owned),
            required: input.instructions_required,
        })?)
    }
}

#[derive(Default, Serialize, Deserialize)]
pub(crate) struct Instructions {
    pub system_prompt: Option<String>,
    pub required: bool,
}

const ANTIGRAVITY_APPEND: &str = "antigravity.systemPrompt.append";

pub(crate) fn creation_meta_key<'a>(
    capabilities: Option<&Meta>,
    configured: Option<&'a str>,
) -> Option<&'a str> {
    if capabilities
        .and_then(|m| m.get("antigravity"))
        .is_some_and(|caps| caps["systemPrompt"]["append"] == true)
    {
        Some(ANTIGRAVITY_APPEND)
    } else {
        configured
    }
}

pub(crate) fn creation_meta(key: &str, text: String) -> Meta {
    let entry = match key {
        ANTIGRAVITY_APPEND => (
            "antigravity".into(),
            json!({"systemPrompt":{"append":text}}),
        ),
        key => (key.to_owned(), Value::String(text)),
    };
    Meta::from_iter([entry])
}
