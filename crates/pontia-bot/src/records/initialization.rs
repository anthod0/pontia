use pontia_agent_control::UpsertExecutionProfileRequest;
use pontia_core::{Error, Result};
use serde_json::json;

use super::{Bot, BotService, DREAMING_PROFILE_ID};

impl BotService {
    /// Retry a saved Bot's initialization without replacing user-managed resources.
    pub async fn initialize(&self, bot_id: &str) -> Result<()> {
        let _guard = self.initialization_gate.lock().await;
        let bot = self.get(bot_id).await?;
        let root = self.home.join("bot");
        let directory = root.join(&bot.bot_id);
        tokio::task::spawn_blocking(move || -> Result<()> {
            std::fs::create_dir_all(directory)?;
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(root.join("USER.md"))
            {
                Ok(_) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    // Existing content must be readable; a directory is not an initialized file.
                    std::fs::read_to_string(root.join("USER.md"))?;
                    Ok(())
                }
                Err(error) => Err(error.into()),
            }
        })
        .await
        .map_err(|error| Error::Domain(error.to_string()))??;
        self.profiles
            .initialize_profile(default_profile(
                &bot,
                Self::helper_profile_id(&bot.bot_id),
                format!("{} helper", bot.name),
            ))
            .await?;
        self.profiles
            .initialize_profile(default_profile(
                &bot,
                DREAMING_PROFILE_ID.into(),
                "Dreaming".into(),
            ))
            .await?;
        let mut retrieval = default_profile(
            &bot,
            Self::retrieval_profile_id(&bot.bot_id),
            format!("{} deep retrieval", bot.name),
        );
        retrieval.system_prompt_template = Some(include_str!("../helpers/retrieval.md").into());
        self.profiles.initialize_profile(retrieval).await?;
        Ok(())
    }
}

fn default_profile(bot: &Bot, profile_id: String, name: String) -> UpsertExecutionProfileRequest {
    UpsertExecutionProfileRequest {
        profile_id,
        version: "1".into(),
        name,
        description: None,
        default_client_type: Some(bot.client_type.clone()),
        default_client_key: Some(bot.client_key.clone()),
        default_model: Some(bot.model.clone()),
        supported_client_types: vec![bot.client_type.clone()],
        agent_kind: "executor".into(),
        system_prompt_template: None,
        turn_prompt_template: None,
        default_session_role: None,
        default_session_description: None,
        handle_prefix: None,
        expected_output_schema: None,
        artifact_contract: json!({}),
        default_execution_policy: json!({}),
        default_review_policy: json!({}),
        metadata: json!({}),
    }
}
