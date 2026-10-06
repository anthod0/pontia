use std::path::Path;

use anyhow::Result;
use clap::Subcommand;
use pontia_edge::{
    config::{CONFIG_PATH, ServiceConfig},
    credential::ensure_root,
    systemd::{Systemd, UNIT_DIRECTORY},
};

#[derive(Subcommand)]
pub enum Command {
    /// Install and enable hourly stable-release updates.
    Enable,
    /// Disable future checks without interrupting an update in progress.
    Disable,
    /// Show the timer schedule and the last update result.
    Status,
}

pub async fn run(command: Command) -> Result<()> {
    let systemd = Systemd::default();
    match command {
        Command::Enable => {
            crate::update::ensure_managed_installation()?;
            ServiceConfig::read(Path::new(CONFIG_PATH))?;
            systemd
                .enable_auto_update(Path::new(UNIT_DIRECTORY))
                .await?;
            println!(
                "Automatic updates enabled: stable releases are checked hourly with up to 10 minutes of random delay."
            );
        }
        Command::Disable => {
            ensure_root(rustix::process::geteuid().as_raw())?;
            systemd.disable_auto_update().await?;
            println!("Automatic updates disabled. Any update already in progress will finish.");
        }
        Command::Status => systemd.show_auto_update().await?,
    }
    Ok(())
}
