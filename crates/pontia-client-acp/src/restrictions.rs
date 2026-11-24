use agent_client_protocol::{
    UntypedMessage,
    v1::schema::{Meta, SessionId},
};
use pontia_core::{Error, Result, SessionMode};
use serde_json::{Value, json};

use crate::peer::{Peer, protocol_error};

#[derive(Debug)]
pub enum RestrictedExecution {
    Antigravity,
    Kiro,
    AgentProfile(&'static AgentProfile),
}

#[derive(Debug)]
pub struct AgentProfile {
    pub profile_key: &'static str,
    pub profile_name: (&'static str, &'static str),
    pub profile_description: (&'static str, &'static str),
    pub disabled_env_flags: &'static [&'static str],
    pub allow_key: &'static str,
    pub allowed_tools: &'static [&'static str],
    pub deny_key: &'static str,
    pub disabled_tools: &'static [&'static str],
    pub discover_skills_key: &'static str,
    pub yolo_key: &'static str,
    pub startup_model_key: &'static str,
    pub list_method: &'static str,
    pub catalog_path: &'static str,
    pub agent_type_path: &'static str,
    pub model_path: &'static str,
    pub allowed_types: &'static [&'static str],
    pub info_method: &'static str,
    pub profile_path: &'static str,
}

impl AgentProfile {
    pub(crate) fn configure_launch(&self, launch: &mut crate::AcpLaunch) {
        for flag in self.disabled_env_flags {
            launch.env.insert((*flag).into(), "false".into());
        }
    }

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
        mode: SessionMode,
    ) -> Result<Self> {
        let mut policy = Self::default();
        match (recipe, mode) {
            (Some(RestrictedExecution::AgentProfile(recipe)), _) => {
                policy
                    .meta
                    .insert(recipe.yolo_key.into(), json!(mode.bypasses_approvals()));
                if mode == SessionMode::Restricted {
                    policy.profile = Some(recipe);
                    policy.meta.insert(
                        recipe.profile_key.into(),
                        json!({
                            (recipe.profile_name.0): recipe.profile_name.1,
                            (recipe.profile_description.0): recipe.profile_description.1,
                            (recipe.allow_key): recipe.allowed_tools,
                            (recipe.deny_key): recipe.disabled_tools,
                            (recipe.discover_skills_key): false,
                        }),
                    );
                }
            }
            (Some(RestrictedExecution::Kiro), SessionMode::Restricted) => {
                policy
                    .meta
                    .insert("kiro".into(), json!({"modeId":"pontia-restricted"}));
            }
            (Some(RestrictedExecution::Kiro), SessionMode::Yolo) => {
                // Kiro keeps session presets in memory, so every new and load request carries it.
                policy
                    .meta
                    .insert("kiro".into(), json!({"policyPreset":["allow-all"]}));
            }
            (
                Some(RestrictedExecution::Antigravity),
                SessionMode::Restricted | SessionMode::Yolo,
            )
            | (_, SessionMode::Default) => {}
            _ => {
                return Err(Error::CapabilityUnavailable(format!(
                    "ACP Agent cannot start in {mode:?} mode"
                )));
            }
        }
        Ok(policy)
    }
}
