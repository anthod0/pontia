use clap::{Args, Subcommand};
use pontia_application::agent_clients::AgentClientDetails;
use pontia_config::AppConfig;
use reqwest::Method;
use serde_json::{Value, json};
use std::path::PathBuf;

const CLIENT_HELP: &str = "\
Requires a running service. Output is JSON.
Select an entry by short ID (pi, codex) or by --id from 'pontia client list'.
Only enabled entries can start new Sessions.

Examples:
  pontia client list
  pontia client enable pi

For Client setup, see https://pontia.dev/docs/agent-clients.md";

const SET_HELP: &str = "\
The JSON object replaces all existing overrides. Supported key: tui_command.

Example:
  echo '{\"tui_command\": \"/usr/local/bin/pi\"}' > pi.json
  pontia client set pi --config pi.json";

#[derive(Debug, Args)]
#[command(after_help = CLIENT_HELP)]
pub struct ClientCommand {
    #[command(subcommand)]
    command: ClientCommandKind,
}

#[derive(Debug, Subcommand)]
enum ClientCommandKind {
    /// List all Agent Client entries
    List,
    /// Show one entry, including its effective configuration
    Show(Target),
    /// Replace an entry's configuration overrides
    #[command(after_help = SET_HELP)]
    Set {
        #[command(flatten)]
        target: Target,
        /// JSON object with the new overrides
        #[arg(long, value_name = "JSON_FILE")]
        config: PathBuf,
    },
    /// Allow new Sessions to use an entry
    Enable(Target),
    /// Stop new Sessions from using an entry
    Disable(Target),
    /// Delete an entry's saved settings, which also disables it
    Reset(Target),
}

#[derive(Debug, Args)]
#[group(required = true, multiple = false)]
struct Target {
    /// Entry short ID, e.g. pi or codex
    client: Option<String>,
    /// Entry ID from 'pontia client list', for ambiguous short IDs
    #[arg(long)]
    id: Option<String>,
}

pub struct ClientApi {
    http: reqwest::Client,
    base: reqwest::Url,
    token: Option<String>,
}

impl ClientApi {
    pub fn new(config: &AppConfig) -> Result<Self, String> {
        let addr = crate::lifecycle::local_health_addr(config.bind_addr);
        Ok(Self {
            http: reqwest::Client::new(),
            base: reqwest::Url::parse(&format!("http://{addr}/api/v1/agent-clients"))
                .map_err(|error| error.to_string())?,
            token: config.external_api_token.clone(),
        })
    }

    async fn request(
        &self,
        method: Method,
        target: Option<(&str, &str)>,
        body: Option<Value>,
    ) -> Result<Value, String> {
        let mut url = self.base.clone();
        if let Some((client, key)) = target {
            url.path_segments_mut()
                .map_err(|_| "Invalid API URL")?
                .push(client)
                .push(key);
        }
        let mut request = self.http.request(method, url);
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request
            .send()
            .await
            .map_err(|error| format!("Client management request failed: {error}"))?;
        let status = response.status();
        let envelope: Value = response
            .json()
            .await
            .map_err(|error| format!("Invalid service response: {error}"))?;
        if !status.is_success() {
            return Err(format!(
                "{status}: {}",
                envelope["error"]["message"]
                    .as_str()
                    .unwrap_or("Client management failed")
            ));
        }
        envelope
            .get("data")
            .cloned()
            .ok_or_else(|| "Service response has no data".into())
    }

    pub async fn list(&self) -> Result<Vec<AgentClientDetails>, String> {
        let data = self.request(Method::GET, None, None).await?;
        serde_json::from_value(data["agent_clients"].clone()).map_err(|error| error.to_string())
    }

    pub async fn patch(&self, client: &str, key: &str, settings: Value) -> Result<Value, String> {
        self.request(Method::PATCH, Some((client, key)), Some(settings))
            .await
    }
}

pub async fn run(command: ClientCommand, config: &AppConfig) -> Result<Value, String> {
    let api = ClientApi::new(config)?;
    let target = match &command.command {
        ClientCommandKind::List => return Ok(json!({"agent_clients": api.list().await?})),
        ClientCommandKind::Show(target)
        | ClientCommandKind::Enable(target)
        | ClientCommandKind::Disable(target)
        | ClientCommandKind::Reset(target)
        | ClientCommandKind::Set { target, .. } => target,
    };
    let entries = api.list().await?;
    let matches = entries
        .iter()
        .filter(|entry| {
            target.id.as_ref().map_or_else(
                || target.client.as_ref() == Some(&entry.short_id),
                |id| id == &entry.id,
            )
        })
        .collect::<Vec<_>>();
    let entry = match matches.as_slice() {
        [entry] => *entry,
        [] => return Err("Client not found; use pontia client list".into()),
        _ => {
            return Err(
                "Ambiguous client selector; use --id with an ID from pontia client list".into(),
            );
        }
    };
    let identity = Some((entry.client_type.as_str(), entry.client_key.as_str()));
    match command.command {
        ClientCommandKind::Show(_) => api.request(Method::GET, identity, None).await,
        ClientCommandKind::Reset(_) => api.request(Method::DELETE, identity, None).await,
        ClientCommandKind::Enable(_) => {
            api.patch(
                &entry.client_type,
                &entry.client_key,
                json!({"enabled": true}),
            )
            .await
        }
        ClientCommandKind::Disable(_) => {
            api.patch(
                &entry.client_type,
                &entry.client_key,
                json!({"enabled": false}),
            )
            .await
        }
        ClientCommandKind::Set { config, .. } => {
            let contents = std::fs::read(&config)
                .map_err(|error| format!("Cannot read {}: {error}", config.display()))?;
            let overrides: Value = serde_json::from_slice(&contents)
                .map_err(|error| format!("Invalid configuration JSON: {error}"))?;
            if !overrides.is_object() {
                return Err("Configuration must be a JSON object".into());
            }
            api.patch(
                &entry.client_type,
                &entry.client_key,
                json!({"config_overrides": overrides}),
            )
            .await
        }
        ClientCommandKind::List => unreachable!(),
    }
}
