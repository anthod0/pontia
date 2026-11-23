use clap::{ArgGroup, Subcommand};
use pontia_bot::conversation::profiles::ProfileClient;
use serde::Serialize;

#[derive(Debug, Subcommand, Serialize)]
#[serde(tag = "action", content = "input", rename_all = "snake_case")]
pub(super) enum ProfileCommand {
    /// List Profiles and their availability for new helpers
    List,
    /// Read a Profile, including its full system prompt
    Show {
        #[arg(long)]
        profile_id: String,
    },
    /// List enabled clients
    Clients,
    /// List the models offered by a client
    Models {
        #[arg(long, value_name = "TYPE/KEY")]
        client: ProfileClient,
    },
    /// Create a shared Agent Profile
    Create {
        #[arg(long)]
        profile_id: String,
        #[arg(long, allow_hyphen_values = true)]
        name: String,
        #[arg(long, allow_hyphen_values = true)]
        description: String,
        #[arg(long, value_name = "TYPE/KEY")]
        client: ProfileClient,
        #[arg(long)]
        model: String,
        #[arg(long, allow_hyphen_values = true)]
        system_prompt: Option<String>,
    },
    /// Change selected fields of a Profile
    #[command(group(ArgGroup::new("changes").args(["name", "description", "clear_description", "system_prompt", "clear_system_prompt", "model"]).required(true).multiple(true)))]
    Update {
        #[arg(long)]
        profile_id: String,
        #[arg(long, allow_hyphen_values = true)]
        name: Option<String>,
        #[arg(long, allow_hyphen_values = true, conflicts_with = "clear_description")]
        description: Option<String>,
        #[arg(long)]
        clear_description: bool,
        #[arg(
            long,
            allow_hyphen_values = true,
            conflicts_with = "clear_system_prompt"
        )]
        system_prompt: Option<String>,
        #[arg(long)]
        clear_system_prompt: bool,
        #[arg(long, value_name = "TYPE/KEY", requires = "model")]
        client: Option<ProfileClient>,
        #[arg(long)]
        model: Option<String>,
    },
    /// Delete a shared Profile
    Delete {
        #[arg(long)]
        profile_id: String,
    },
}
