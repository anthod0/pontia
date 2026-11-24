use pontia_core::{Error, Result};

use super::BotService;

impl BotService {
    /// Prepare the Bot's persistent launch directory and shared user file.
    pub async fn initialize(&self, bot_id: &str) -> Result<()> {
        let _guard = self.initialization_gate.lock().await;
        let bot = self.get(bot_id).await?;
        let root = self.home.join("bot");
        let directory = root.join(&bot.bot_id);
        let clients = self.clients.clone();
        tokio::task::spawn_blocking(move || -> Result<()> {
            clients.initialize_private_directory(&directory)?;
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
        .map_err(|error| Error::Domain(error.to_string()))?
    }
}
