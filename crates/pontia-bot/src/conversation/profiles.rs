use super::ConversationService;
use pontia_agent_control::{
    AgentProfileService, UpsertAgentProfileRequest, agent_clients::AgentClientService,
    client_contract::DetectionContext,
};
use pontia_core::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::str::FromStr;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileClient {
    client_type: String,
    client_key: String,
}

impl FromStr for ProfileClient {
    type Err = &'static str;

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        let (client_type, client_key) = value.split_once('/').ok_or("expected TYPE/KEY")?;
        if client_type.trim().is_empty() || client_key.trim().is_empty() {
            return Err("expected TYPE/KEY");
        }
        Ok(Self {
            client_type: client_type.into(),
            client_key: client_key.into(),
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileRequest {
    pub session_id: String,
    command: ProfileCommand,
}

#[derive(Deserialize)]
#[serde(
    tag = "action",
    content = "input",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum ProfileCommand {
    List,
    Show {
        profile_id: String,
    },
    Clients,
    Models {
        client: ProfileClient,
    },
    Create {
        profile_id: String,
        name: String,
        description: String,
        client: ProfileClient,
        model: String,
        system_prompt: Option<String>,
    },
    Update(ProfileUpdate),
    Delete {
        profile_id: String,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProfileUpdate {
    profile_id: String,
    name: Option<String>,
    description: Option<String>,
    #[serde(default)]
    clear_description: bool,
    system_prompt: Option<String>,
    #[serde(default)]
    clear_system_prompt: bool,
    client: Option<ProfileClient>,
    model: Option<String>,
}

impl ProfileUpdate {
    async fn merge(self, profiles: &AgentProfileService) -> Result<UpsertAgentProfileRequest> {
        if self.clear_description && self.description.is_some()
            || self.clear_system_prompt && self.system_prompt.is_some()
        {
            return Err(Error::Domain(
                "Cannot set and clear a field together".into(),
            ));
        }
        if self.client.is_some() && self.model.is_none() {
            return Err(Error::Domain("Changing client requires a model".into()));
        }
        if self.name.is_none()
            && self.description.is_none()
            && !self.clear_description
            && self.system_prompt.is_none()
            && !self.clear_system_prompt
            && self.model.is_none()
        {
            return Err(Error::Domain("Specify a Profile field to update".into()));
        }
        let current = profiles.require_profile(&self.profile_id).await?;
        let (client_type, client_key) = match self.client {
            Some(client) => (Some(client.client_type), Some(client.client_key)),
            None => (current.client_type, current.client_key),
        };
        Ok(UpsertAgentProfileRequest {
            profile_id: self.profile_id,
            name: self.name.unwrap_or(current.name),
            client_type,
            client_key,
            model: self.model.or(current.model),
            description: if self.clear_description {
                None
            } else {
                self.description.or(current.description)
            },
            system_prompt: if self.clear_system_prompt {
                None
            } else {
                self.system_prompt.or(current.system_prompt)
            },
        })
    }
}

impl ConversationService {
    pub async fn profile_command(&self, request: ProfileRequest) -> Result<Value> {
        self.require_bot(&request.session_id).await?;
        let source = Some(request.session_id.as_str());
        let profiles = self.control.agent_profiles();
        let clients = AgentClientService::new(self.control.db(), self.control.clients());
        let home = self.control.pontia_home();
        match request.command {
            ProfileCommand::List => self.list_profiles(&profiles).await,
            ProfileCommand::Clients => {
                let vars = std::env::vars().collect();
                let entries = clients
                    .list(&DetectionContext {
                        vars: &vars,
                        pontia_home: home,
                    })
                    .await?;
                let enabled: Vec<_> = entries.into_iter().filter(|entry| entry.enabled).map(|entry| {
                    json!({"client": format!("{}/{}", entry.client_type, entry.client_key), "name": entry.name})
                }).collect();
                Ok(json!({"clients": enabled}))
            }
            ProfileCommand::Models { client } => Ok(json!(
                clients
                    .list_models(&client.client_type, &client.client_key, home)
                    .await?
            )),
            ProfileCommand::Show { profile_id } => {
                Ok(json!({"profile": profiles.require_profile(&profile_id).await?}))
            }
            ProfileCommand::Create {
                profile_id,
                name,
                description,
                client,
                model,
                system_prompt,
            } => {
                if description.trim().is_empty() {
                    return Err(Error::Domain("description cannot be empty".into()));
                }
                let input = UpsertAgentProfileRequest {
                    profile_id,
                    name,
                    description: Some(description),
                    client_type: Some(client.client_type),
                    client_key: Some(client.client_key),
                    model: Some(model),
                    system_prompt,
                };
                self.validate_profile_model(&clients, &input).await?;
                Ok(json!({"profile": profiles.create(input, source).await?}))
            }
            ProfileCommand::Update(update) => {
                let changes_model = update.model.is_some();
                let input = update.merge(&profiles).await?;
                if changes_model {
                    self.validate_profile_model(&clients, &input).await?;
                }
                let id = input.profile_id.clone();
                Ok(json!({"profile": profiles.update(&id, input, source).await?}))
            }
            ProfileCommand::Delete { profile_id } => {
                profiles.delete(&profile_id, source).await?;
                Ok(json!({"profile_id": profile_id}))
            }
        }
    }

    async fn validate_profile_model(
        &self,
        clients: &AgentClientService,
        input: &UpsertAgentProfileRequest,
    ) -> Result<()> {
        let (Some(client), Some(key), Some(model)) =
            (&input.client_type, &input.client_key, &input.model)
        else {
            return Err(Error::Domain(
                "Choose a client and model for the Profile".into(),
            ));
        };
        let catalog = clients
            .list_models(client, key, self.control.pontia_home())
            .await?;
        if !catalog.models.iter().any(|entry| entry.id == *model) {
            return Err(Error::Domain(format!(
                "Model {model} is unavailable for {client}/{key}"
            )));
        }
        Ok(())
    }

    async fn list_profiles(&self, profiles: &AgentProfileService) -> Result<Value> {
        let mut items = Vec::new();
        for profile in profiles.list().await? {
            let client = profile
                .client_type
                .as_ref()
                .zip(profile.client_key.as_ref());
            let reason = match client {
                Some((client, key)) => match profiles
                    .launch_client_unavailable_reason(client, key)
                    .await?
                {
                    Some(reason) => Some(reason),
                    None => self
                        .require_helper_client(client, key)
                        .err()
                        .map(|err| err.to_string()),
                },
                None => Some("No launch defaults".into()),
            };
            items.push(json!({
                "profile_id": profile.profile_id, "name": profile.name, "description": profile.description,
                "client": client.map(|(client, key)| format!("{client}/{key}")), "model": profile.model,
                "has_system_prompt": profile.system_prompt.is_some(), "available": reason.is_none(),
                "unavailable_reason": reason, "updated_at": profile.updated_at,
            }));
        }
        Ok(json!({"profiles": items}))
    }
}
