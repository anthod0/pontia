use agent_client_protocol::{
    UntypedMessage,
    v1::schema::{Meta, SessionId},
};
use pontia_core::{Error, Result, execution_policy::ExecutionPolicy};
use serde_json::{Value, json};

use crate::peer::{Peer, protocol_error};

#[derive(Debug)]
pub enum RestrictedExecution {
    Antigravity,
    AgentProfile(&'static AgentProfile),
}

#[derive(Debug)]
pub struct AgentProfile {
    pub profile_key: &'static str,
    pub profile_name: (&'static str, &'static str),
    pub profile_description: (&'static str, &'static str),
    pub deny_key: &'static str,
    pub disabled_tools: &'static [&'static str],
    pub deny_dependencies: &'static [(&'static str, &'static str)],
    pub discover_skills_key: Option<&'static str>,
    pub yolo_key: Option<&'static str>,
    pub startup_model_key: &'static str,
    pub list_method: &'static str,
    pub catalog_path: &'static str,
    pub agent_type_path: &'static str,
    pub model_path: &'static str,
    pub allowed_types: &'static [&'static str],
    pub info_method: &'static str,
    pub profile_path: &'static str,
}

impl RestrictedExecution {
    pub(crate) fn policy(&self) -> ExecutionPolicy {
        let disabled_tools = match self {
            Self::AgentProfile(profile) => profile.disabled_tools,
            Self::Antigravity => crate::antigravity::DISABLED_TOOLS,
        };
        ExecutionPolicy {
            disabled_tools: disabled_tools.iter().map(|s| (*s).into()).collect(),
            ..Default::default()
        }
    }
}

impl AgentProfile {
    pub(crate) async fn check_model(&self, peer: &Peer, requested: Option<&str>) -> Result<String> {
        let cwd = peer.state.lock().await.cwd.clone();
        let result = peer
            .call(
                UntypedMessage::new(self.list_method, json!({"cwd":cwd}))
                    .map_err(protocol_error)?,
            )
            .await?;
        let catalog = result
            .pointer(self.catalog_path)
            .ok_or_else(|| Error::CapabilityUnavailable("ACP model catalog unavailable".into()))?;
        let id = requested
            .or_else(|| catalog["currentModelId"].as_str())
            .ok_or_else(|| Error::CapabilityUnavailable("ACP default model unavailable".into()))?;
        let allowed = catalog["availableModels"]
            .as_array()
            .and_then(|models| {
                models
                    .iter()
                    .find(|model| model["modelId"].as_str() == Some(id))
            })
            .and_then(|model| model.pointer(self.agent_type_path)?.as_str())
            .is_some_and(|kind| self.allowed_types.contains(&kind));
        if !allowed {
            return Err(Error::CapabilityUnavailable(format!(
                "ACP model {id} lacks a verified type for restricted Sessions"
            )));
        }
        Ok(id.to_owned())
    }

    pub(crate) async fn verify(&self, peer: &Peer, native: &SessionId) -> Result<()> {
        let result = peer
            .call(
                UntypedMessage::new(self.info_method, json!({"sessionId":native}))
                    .map_err(protocol_error)?,
            )
            .await?;
        if result.pointer(self.profile_path).and_then(Value::as_str) != Some(self.profile_name.1) {
            return Err(Error::CapabilityUnavailable(
                "ACP Session did not confirm its restricted profile".into(),
            ));
        }
        let model = result
            .pointer(self.model_path)
            .and_then(Value::as_str)
            .ok_or_else(|| {
                Error::CapabilityUnavailable(
                    "ACP Session did not report its effective model".into(),
                )
            })?;
        let verified = self.check_model(peer, Some(model)).await?;
        peer.state.lock().await.verified_model = Some(verified);
        Ok(())
    }
}

#[derive(Clone, Default)]
pub(crate) struct SessionPolicy {
    pub profile: Option<&'static AgentProfile>,
    pub meta: Meta,
}

impl SessionPolicy {
    pub fn resolve(
        recipe: Option<&'static RestrictedExecution>,
        policy: &ExecutionPolicy,
    ) -> Result<Self> {
        let Some(recipe) = recipe else {
            return if policy == &ExecutionPolicy::default() {
                Ok(Self::default())
            } else {
                Err(Error::CapabilityUnavailable(
                    "ACP Agent lacks the requested execution policy".into(),
                ))
            };
        };
        let RestrictedExecution::AgentProfile(recipe) = recipe else {
            crate::antigravity::validate(policy)?;
            return Ok(Self::default());
        };
        if policy.tools.is_some()
            || policy.disable_mcp
            || (policy.disable_skills && recipe.discover_skills_key.is_none())
            || (policy.yolo && recipe.yolo_key.is_none())
        {
            return Err(Error::CapabilityUnavailable(
                "ACP Agent cannot enforce the requested execution policy".into(),
            ));
        }
        if let Some(name) = policy
            .disabled_tools
            .iter()
            .find(|name| !recipe.disabled_tools.contains(&name.as_str()))
        {
            return Err(Error::CapabilityUnavailable(format!(
                "ACP Agent cannot disable tool {name}"
            )));
        }
        let disabled = |name: &str| policy.disabled_tools.iter().any(|tool| tool == name);
        if let Some((tool, required)) = recipe
            .deny_dependencies
            .iter()
            .find(|(tool, required)| disabled(tool) && !disabled(required))
        {
            return Err(Error::CapabilityUnavailable(format!(
                "Disabling {tool} also requires disabling {required}"
            )));
        }
        let mut meta = Meta::new();
        if let Some(key) = recipe.yolo_key {
            meta.insert(key.into(), json!(policy.yolo));
        }
        let restricted = policy.disable_skills || !policy.disabled_tools.is_empty();
        if restricted {
            let mut profile = json!({(recipe.profile_name.0):recipe.profile_name.1, (recipe.profile_description.0):recipe.profile_description.1, (recipe.deny_key):policy.disabled_tools});
            if policy.disable_skills
                && let Some(key) = recipe.discover_skills_key
            {
                profile[key] = json!(false);
            }
            meta.insert(recipe.profile_key.into(), profile);
        }
        Ok(Self {
            profile: restricted.then_some(recipe),
            meta,
        })
    }
}
