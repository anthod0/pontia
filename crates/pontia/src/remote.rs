use std::{collections::HashMap, fs, io::ErrorKind, path::Path, time::Duration};

use clap::{Args, Subcommand};
use dialoguer::{Input, Select};
use reqwest::{Client, StatusCode, Url};
use serde::{Deserialize, Serialize};
use toml_edit::{DocumentMut, Item};
use uuid::Uuid;

use crate::login;
use pontia::private_file;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Args)]
pub(crate) struct RemoteCommand {
    #[command(subcommand)]
    command: RemoteCommandKind,
}

#[derive(Debug, Subcommand)]
enum RemoteCommandKind {
    /// Register this machine as a remote device
    Enable,
}

pub(crate) async fn run(
    command: RemoteCommand,
    vars: &HashMap<String, String>,
) -> Result<(), String> {
    match command.command {
        RemoteCommandKind::Enable => enable(vars).await,
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredCredential {
    token: String,
}

#[derive(Debug, Deserialize)]
struct Edge {
    id: String,
    name: String,
}

#[derive(Debug, Deserialize)]
struct Device {
    id: String,
    name: Option<String>,
    edge_id: String,
    edge_name: String,
}

#[derive(Serialize)]
struct RegistrationRequest<'a> {
    name: &'a str,
    edge_id: &'a str,
}

struct RegistrationApi<'a> {
    client: &'a Client,
    origin: &'a Url,
    credential: &'a str,
}

pub async fn enable(vars: &HashMap<String, String>) -> Result<(), String> {
    let home = login::pontia_home(vars)?;
    let origin = login::auth_origin(vars)?;
    let credential = read_credential(&home.join("auth.json"))?;
    let client = Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(|error| {
            format!("failed to create the remote registration HTTP client: {error}")
        })?;
    let api = RegistrationApi {
        client: &client,
        origin: &origin,
        credential: &credential,
    };
    let config_path = home.join("config.toml");
    let device_id = ensure_device_id(&config_path)?;

    if let Some(device) = api.find_device(&device_id).await? {
        print_registered(&device, true);
        return Ok(());
    }

    let edges = api.fetch_edges().await?;
    let edge = select_edge(&edges)?;
    let default_name = hostname::get()
        .map_err(|error| format!("failed to read the local hostname: {error}"))?
        .to_string_lossy()
        .trim()
        .to_string();
    if default_name.is_empty() {
        return Err(
            "the local hostname is empty; set a device name after fixing the hostname".to_string(),
        );
    }
    let name = Input::<String>::new()
        .with_prompt("Device name")
        .default(default_name)
        .interact_text()
        .map_err(|error| format!("failed to read the device name: {error}"))?;
    validate_name(&name)?;

    let device = api.register_device(&device_id, &name, &edge.id).await?;
    print_registered(&device, false);
    Ok(())
}

fn read_credential(path: &Path) -> Result<String, String> {
    let contents = fs::read(path).map_err(|error| {
        if error.kind() == ErrorKind::NotFound {
            "not logged in; run `pontia login` before enabling remote access".to_string()
        } else {
            format!("failed to read {}: {error}", path.display())
        }
    })?;
    let stored: StoredCredential = serde_json::from_slice(&contents).map_err(|_| {
        format!(
            "invalid login credential in {}; run `pontia login` again",
            path.display()
        )
    })?;
    login::validate_token(&stored.token).map_err(|_| {
        format!(
            "invalid login credential in {}; run `pontia login` again",
            path.display()
        )
    })?;
    Ok(stored.token)
}

fn ensure_device_id(path: &Path) -> Result<String, String> {
    let original = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == ErrorKind::NotFound => String::new(),
        Err(error) => return Err(format!("failed to read {}: {error}", path.display())),
    };
    let mut document = if original.trim().is_empty() {
        DocumentMut::new()
    } else {
        original
            .parse::<DocumentMut>()
            .map_err(|error| format!("failed to parse {}: {error}", path.display()))?
    };
    if let Some(value) = document
        .get("remote")
        .and_then(Item::as_table_like)
        .and_then(|remote| remote.get("device_id"))
        .and_then(Item::as_str)
    {
        if !uuid_v7(value) {
            return Err(format!(
                "remote.device_id in {} must be a UUIDv7",
                path.display()
            ));
        }
        return Ok(value.to_string());
    }
    if document
        .get("remote")
        .and_then(Item::as_table_like)
        .and_then(|remote| remote.get("device_id"))
        .is_some()
    {
        return Err(format!(
            "remote.device_id in {} must be a UUIDv7 string",
            path.display()
        ));
    }

    let device_id = Uuid::now_v7().to_string();
    document["remote"]["device_id"] = toml_edit::value(&device_id);
    private_file::atomic_write(path, document.to_string().as_bytes())?;
    Ok(device_id)
}

fn uuid_v7(value: &str) -> bool {
    Uuid::parse_str(value)
        .is_ok_and(|parsed| parsed.get_version_num() == 7 && parsed.to_string() == value)
}

fn validate_name(name: &str) -> Result<(), String> {
    if name.is_empty() || name.trim() != name || name.chars().any(char::is_control) {
        return Err("device name must be non-empty and contain no surrounding whitespace or control characters".to_string());
    }
    Ok(())
}

impl RegistrationApi<'_> {
    async fn find_device(&self, device_id: &str) -> Result<Option<Device>, String> {
        let response = self
            .client
            .get(endpoint(
                self.origin,
                &format!("api/remote/devices/{device_id}"),
            )?)
            .bearer_auth(self.credential)
            .send()
            .await
            .map_err(|error| uncertain("query device registration", error))?;
        match response.status() {
            StatusCode::OK => parse_device(response, device_id).await.map(Some),
            StatusCode::NOT_FOUND => Ok(None),
            StatusCode::UNAUTHORIZED => Err(login_required()),
            StatusCode::CONFLICT => Err(
                "the configured device ID is already registered to a different user; refusing to replace it"
                    .to_string(),
            ),
            status => Err(format!(
                "the website rejected the device registration query ({status}); the configured device ID was preserved"
            )),
        }
    }

    async fn fetch_edges(&self) -> Result<Vec<Edge>, String> {
        let response = self
            .client
            .get(endpoint(self.origin, "api/remote/edges")?)
            .bearer_auth(self.credential)
            .send()
            .await
            .map_err(|error| uncertain("fetch official edges", error))?;
        if response.status() == StatusCode::UNAUTHORIZED {
            return Err(login_required());
        }
        if !response.status().is_success() {
            return Err(format!(
                "the website rejected the official edge request ({})",
                response.status()
            ));
        }
        let edges: Vec<Edge> = response
            .json()
            .await
            .map_err(|_| "the website returned an invalid official edge list".to_string())?;
        if edges.is_empty() {
            return Err("no official edge is currently available for registration".to_string());
        }
        if edges
            .iter()
            .any(|edge| !uuid_v7(&edge.id) || edge.name.is_empty())
        {
            return Err("the website returned an invalid official edge list".to_string());
        }
        Ok(edges)
    }
}

fn select_edge(edges: &[Edge]) -> Result<&Edge, String> {
    if edges.len() == 1 {
        println!("Using official edge: {}", edges[0].name);
        return Ok(&edges[0]);
    }
    let names: Vec<&str> = edges.iter().map(|edge| edge.name.as_str()).collect();
    let selected = Select::new()
        .with_prompt("Official edge")
        .items(&names)
        .default(0)
        .interact()
        .map_err(|error| format!("failed to select an official edge: {error}"))?;
    Ok(&edges[selected])
}

impl RegistrationApi<'_> {
    async fn register_device(
        &self,
        device_id: &str,
        name: &str,
        edge_id: &str,
    ) -> Result<Device, String> {
        let response = self
            .client
            .put(endpoint(
                self.origin,
                &format!("api/remote/devices/{device_id}"),
            )?)
            .bearer_auth(self.credential)
            .json(&RegistrationRequest { name, edge_id })
            .send()
            .await
            .map_err(|error| uncertain("register the device", error))?;
        match response.status() {
            StatusCode::OK | StatusCode::CREATED => parse_device(response, device_id).await,
            StatusCode::UNAUTHORIZED => Err(login_required()),
            StatusCode::CONFLICT => Err(
                "the selected edge is no longer available or the device registration conflicts with an existing device; run the command again to check its status"
                    .to_string(),
            ),
            StatusCode::BAD_REQUEST => {
                Err("the website rejected the device registration data".to_string())
            }
            status => Err(format!(
                "the website rejected device registration ({status}); the configured device ID was preserved"
            )),
        }
    }
}

async fn parse_device(response: reqwest::Response, expected_id: &str) -> Result<Device, String> {
    let device: Device = response.json().await.map_err(|_| uncertain_result())?;
    if device.id != expected_id || !uuid_v7(&device.id) || !uuid_v7(&device.edge_id) {
        return Err(uncertain_result());
    }
    Ok(device)
}

fn endpoint(origin: &Url, path: &str) -> Result<Url, String> {
    origin
        .join(path)
        .map_err(|error| format!("invalid remote registration endpoint: {error}"))
}

fn uncertain(action: &str, error: reqwest::Error) -> String {
    format!(
        "failed to {action}: {error}; the result is unknown and the configured device ID was preserved"
    )
}

fn uncertain_result() -> String {
    "the website returned an invalid device registration result; the registration state is unknown and the configured device ID was preserved".to_string()
}

fn login_required() -> String {
    "the login credential is invalid; run `pontia login` again".to_string()
}

fn print_registered(device: &Device, already_registered: bool) {
    let name = device.name.as_deref().unwrap_or("unnamed device");
    if already_registered {
        println!("Device is already registered.");
    } else {
        println!("Device registration complete.");
    }
    println!("  Device: {name}");
    println!("  Device ID: {}", device.id);
    println!("  Edge: {}", device.edge_name);
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn device_id_is_written_once_without_replacing_other_configuration() {
        let root = tempdir().unwrap();
        let path = root.path().join("config.toml");
        fs::write(&path, "bind_addr = '127.0.0.1:9000'\n").unwrap();

        let first = ensure_device_id(&path).unwrap();
        let second = ensure_device_id(&path).unwrap();
        let contents = fs::read_to_string(path).unwrap();

        assert_eq!(first, second);
        assert!(uuid_v7(&first));
        assert!(contents.contains("bind_addr = '127.0.0.1:9000'"));
        assert!(contents.contains(&format!("device_id = \"{first}\"")));
    }

    #[test]
    fn invalid_stored_device_id_is_not_replaced() {
        let root = tempdir().unwrap();
        let path = root.path().join("config.toml");
        let original = "[remote]\ndevice_id = 'not-a-uuid'\n";
        fs::write(&path, original).unwrap();

        assert!(ensure_device_id(&path).unwrap_err().contains("UUIDv7"));
        assert_eq!(fs::read_to_string(path).unwrap(), original);
    }
}
