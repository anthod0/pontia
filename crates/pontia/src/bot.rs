use std::{
    env,
    fs::File,
    io::{self, Read},
    path::PathBuf,
    time::Duration,
};

use clap::{Args, Subcommand, ValueEnum};
use pontia_config::AppConfig;
use serde::Serialize;
use serde_json::{Value, json};

#[derive(Debug, Args)]
pub(crate) struct BotCommand {
    #[command(subcommand)]
    command: BotTool,
}

#[derive(Debug, Subcommand)]
enum BotTool {
    /// Read or maintain device memories using JSON input
    #[command(
        after_help = "Input is a JSON object for the selected action, read from stdin by default.\nRequires PONTIA_SESSION_ID and a Session authorized by the Bot application.\nPrints the JSON result; errors exit nonzero.\n\nExample search input: {\"keywords\":[\"项目\",\"Rust\"],\"limit\":10}\nExample create input: {\"kind\":\"fact\",\"content\":\"项目使用 Rust\",\"keywords\":[\"项目\",\"Rust\"]}\n\nKeywords: 1–32 strings, each 1–128 characters after normalization.\nWhitespace is collapsed, letters lowercased, duplicates removed.\nEach keyword matches as a whole, including Chinese short words and mixed text.\nUse read/use with ids; archive with id and archived; link/unlink with memory_a_id\nand memory_b_id; neighbors with id, optional after cursor and limit (1–100).\nUpdate replaces kind, content, keywords, importance, occurred_at and sources.\nSearch limit is 1–30 (default 10); has_more refers only to the top 100 candidates.\nRead/use accept 1–100 IDs. Optional workspace_id restricts reads to that Workspace."
    )]
    Memory {
        #[arg(value_enum)]
        action: MemoryAction,
        /// JSON input file, or - for stdin
        #[arg(long, default_value = "-")]
        input: PathBuf,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum, Serialize)]
#[serde(rename_all = "snake_case")]
enum MemoryAction {
    Create,
    Update,
    Archive,
    Link,
    Unlink,
    Read,
    Search,
    Neighbors,
    Use,
}

pub(crate) async fn run(command: BotCommand, config: &AppConfig) -> Result<(), String> {
    let session_id = env::var("PONTIA_SESSION_ID")
        .ok()
        .filter(|id| !id.trim().is_empty())
        .ok_or("Missing PONTIA_SESSION_ID; run bot tools inside a Pontia Session")?;
    let BotTool::Memory { action, input } = command.command;
    let source: Box<dyn Read> = if input.as_os_str() == "-" {
        Box::new(io::stdin())
    } else {
        Box::new(
            File::open(&input).map_err(|err| format!("Cannot read {}: {err}", input.display()))?,
        )
    };
    let mut bytes = Vec::new();
    source
        .take(2 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|err| err.to_string())?;
    if bytes.len() > 2 * 1024 * 1024 {
        return Err("Memory input exceeds 2 MiB".into());
    }
    let input: Value =
        serde_json::from_slice(&bytes).map_err(|err| format!("Invalid JSON input: {err}"))?;
    if !input.is_object() {
        return Err("Memory input must be a JSON object".into());
    }
    let token = config
        .external_api_token
        .as_deref()
        .ok_or("Pontia local API token is not configured")?;
    let addr = pontia::lifecycle::local_health_addr(config.bind_addr);
    let response = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|err| err.to_string())?
        .post(format!("http://{addr}/api/v1/bot/memory"))
        .bearer_auth(token)
        .json(&json!({"session_id": session_id, "command": {"action": action, "input": input}}))
        .send()
        .await
        .map_err(|err| format!("Cannot reach pontiad: {err}"))?;
    let status = response.status();
    let body: Value = response
        .json()
        .await
        .map_err(|err| format!("Invalid service response: {err}"))?;
    if !status.is_success() {
        return Err(format!("{status}: {}", body["error"]));
    }
    let data = body
        .get("data")
        .filter(|data| !data.is_null())
        .ok_or("Service response has no data")?;
    println!(
        "{}",
        serde_json::to_string(data).map_err(|err| err.to_string())?
    );
    Ok(())
}
