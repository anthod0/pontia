mod args;
mod output;

use std::{env, time::Duration};

use pontia_bot::identity::{TOOL_TOKEN_ENV, TOOL_TOKEN_HEADER};
use pontia_config::AppConfig;
use serde::Deserialize;
use serde_json::{Value, json};

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

pub(crate) async fn run(command: BotCommand, config: &AppConfig) -> Result<(), String> {
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
        BotTool::Workspace(input) => ("workspace", json!(input), true),
        BotTool::SessionMessages(input) => ("session-messages", json!(input), false),
        BotTool::UpdateUser(input) => ("user", json!(input), false),
        BotTool::LinkMessage {
            message_id,
            related_to,
        } => (
            "link-message",
            json!({ "message_id": message_id, "related_to": related_to }),
            false,
        ),
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
    let token = config
        .external_api_token
        .as_deref()
        .ok_or("Pontia local API token is not configured")?;
    let addr = pontia::lifecycle::local_health_addr(config.bind_addr);
    let mut request = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|err| err.to_string())?
        .post(format!("http://{addr}/api/v1/bot/{endpoint}"))
        .bearer_auth(token)
        .json(&request);
    if let Ok(credential) = env::var(TOOL_TOKEN_ENV) {
        request = request.header(TOOL_TOKEN_HEADER, credential);
    }
    let response = request
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
