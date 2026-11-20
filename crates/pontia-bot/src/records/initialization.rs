use pontia_agent_control::UpsertAgentProfileRequest;
use pontia_core::{Error, Result};
use serde_json::json;

use super::{Bot, BotService};

impl BotService {
    /// Retry a saved Bot's initialization without replacing user-managed resources.
    pub async fn initialize(&self, bot_id: &str) -> Result<()> {
        let _guard = self.initialization_gate.lock().await;
        let bot = self.get(bot_id).await?;
        let root = self.home.join("bot");
        let directory = root.join(&bot.bot_id);
        tokio::task::spawn_blocking(move || -> Result<()> {
            let pi = directory.join(".pi");
            std::fs::create_dir_all(&pi)?;
            // Temporary until Pi exposes steeringMode as a CLI launch option.
            let settings = tempfile::NamedTempFile::new_in(&pi)?;
            serde_json::to_writer(settings.as_file(), &json!({"steeringMode": "all"}))?;
            match settings.persist_noclobber(pi.join("settings.json")) {
                Ok(_) => (),
                Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => (),
                Err(error) => return Err(error.error.into()),
            }
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
            .initialize_profile(helper_profile(&bot))
            .await?;
        Ok(())
    }
}

fn helper_profile(bot: &Bot) -> UpsertAgentProfileRequest {
    UpsertAgentProfileRequest {
        profile_id: BotService::helper_profile_id(&bot.bot_id),
        name: format!("{} helper", bot.name),
        description: None,
        client_type: Some(bot.client_type.clone()),
        client_key: Some(bot.client_key.clone()),
        model: Some(bot.model.clone()),
        system_prompt: None,
    }
}
