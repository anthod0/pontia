mod args;
mod output;
mod profile;
mod profile_templates;

use std::{env, time::Duration};

use pontia_bot::identity::{TOOL_TOKEN_ENV, TOOL_TOKEN_HEADER};
use serde::Deserialize;
use serde_json::{Value, json};
use std::net::SocketAddr;

pub(crate) use args::BotCommand;
use args::{BotTool, MemoryCommand};

#[derive(Deserialize)]
struct ApiResponse {
    data: Option<Value>,
    error: Option<ApiError>,
}

#[derive(Deserialize)]
struct ApiError {
    code: String,
    message: String,
}

pub(crate) async fn run(command: BotCommand) -> Result<(), String> {
    if matches!(
        command.command,
        BotTool::Profile(profile::ProfileCommand::Templates)
    ) {
        let vars = env::var("PONTIA_AUTH_ORIGIN")
            .into_iter()
            .map(|origin| ("PONTIA_AUTH_ORIGIN".into(), origin))
            .collect();
        let origin = pontia_config::auth_origin(&vars).map_err(|error| error.to_string())?;
        print!("{}", profile_templates::load(&origin, command.json).await);
        return Ok(());
    }
    let session_id = env::var("PONTIA_SESSION_ID")
        .ok()
        .filter(|id| !id.trim().is_empty())
        .ok_or("Missing PONTIA_SESSION_ID; run bot tools inside a Pontia Session")?;
    let mut reason = None;
    let (endpoint, input, grouped) = match command.command {
        BotTool::Dreaming(input) => ("dreaming", json!(input), true),
        BotTool::Helper(input) => ("helper", json!(input), true),
        BotTool::Memory(input) => {
            let endpoint = match input.command {
                MemoryCommand::Retrieve { .. } => "helper",
                _ => "memory",
            };
            reason = input.reason;
            (endpoint, json!(input.command), true)
        }
        BotTool::Profile(input) => ("profile", json!(input), true),
        BotTool::Workspace(input) => ("workspace", json!(input), true),
        BotTool::ChatHistory(input) => ("chat-history", json!(input), false),
        BotTool::SessionMessages(input) => ("session-messages", json!(input), false),
        BotTool::UpdateUser(input) => ("user", json!(input), false),
        BotTool::SetDescription { description } => {
            ("description", json!({ "description": description }), false)
        }
        BotTool::LinkMessage {
            message_id,
            related_to,
        } => (
            "link-message",
            json!({ "message_id": message_id, "related_to": related_to }),
            false,
        ),
        BotTool::Todo(command) => ("todo", json!(command), true),
        BotTool::SendToUser(input) => ("send-to-user", json!(input), false),
    };
    let mut request = if grouped {
        json!({"session_id": session_id, "command": input})
    } else {
        json!({"session_id": session_id, "input": input})
    };
    if endpoint == "memory" {
        request["reason"] = json!(reason);
    }
    let token = env::var(TOOL_TOKEN_ENV).map_err(|_| "Missing PONTIA_BOT_TOOL_TOKEN")?;
    let addr: SocketAddr = env::var(pontia_bot::identity::TOOL_ADDR_ENV)
        .map_err(|_| "Missing PONTIA_BOT_API_ADDR")?
        .parse()
        .map_err(|_| "Invalid PONTIA_BOT_API_ADDR")?;
    let response = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|err| err.to_string())?
        .post(format!("http://{addr}/api/v1/bot/{endpoint}"))
        .header(TOOL_TOKEN_HEADER, token)
        .json(&request)
        .send()
        .await
        .map_err(|err| format!("Cannot reach pontiad: {err}"))?;
    let status = response.status();
    let body: ApiResponse = response
        .json()
        .await
        .map_err(|err| format!("Invalid service response: {err}"))?;
    if !status.is_success() {
        let error = body.error.ok_or("Service response has no error details")?;
        return Err(format!("{status}: {}: {}", error.code, error.message));
    }
    let data = body.data.ok_or("Service response has no data")?;
    if command.json {
        println!(
            "{}",
            serde_json::to_string(&data).map_err(|err| err.to_string())?
        );
    } else {
        print!("{}", output::plain_text(&data));
        if let Some(next) = output::continuation(endpoint, &input, &data) {
            println!("Next page:\n{next}");
        }
    }
    Ok(())
}
