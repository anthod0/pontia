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
            let user = root.join("USER.md");
            match pontia_core::private_fs::open(
                &user,
                std::fs::OpenOptions::new().write(true).create_new(true),
            ) {
                Ok(_) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    let mut file = pontia_core::private_fs::open(
                        &user,
                        std::fs::OpenOptions::new().read(true),
                    )?;
                    std::io::Read::read_to_string(&mut file, &mut String::new())?;
                    Ok(())
                }
                Err(error) => Err(error.into()),
            }
        })
        .await
        .map_err(|error| Error::Domain(error.to_string()))?
    }
}
