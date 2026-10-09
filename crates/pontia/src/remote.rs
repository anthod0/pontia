use std::{collections::HashMap, fs, io::ErrorKind, path::Path, time::Duration};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use clap::{Args, Subcommand};
use dialoguer::{Input, Select};
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use pontia_e2e::DeviceIdentity;
use reqwest::{Client, StatusCode, Url};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use toml_edit::{DocumentMut, Item};
use uuid::Uuid;
use x25519_dalek::{PublicKey as X25519PublicKey, StaticSecret};

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
    /// Replace the device E2E identity key
    RotateKey,
    /// Unregister this machine and disable remote access
    Disable,
}

pub(crate) async fn run(
    command: RemoteCommand,
    vars: &HashMap<String, String>,
) -> Result<(), String> {
    match command.command {
        RemoteCommandKind::Enable => enable(vars).await.map(|_| ()),
        RemoteCommandKind::RotateKey => rotate_key(vars).await,
        RemoteCommandKind::Disable => disable(vars).await,
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
    #[serde(rename = "device_handle")]
    handle: String,
    name: Option<String>,
    edge_id: String,
    edge_name: String,
    e2e_public_key: String,
    e2e_key_version: u64,
    capability_verification_key: String,
}

pub(crate) struct RemoteAccess {
    pub(crate) device_handle: String,
}

#[derive(Serialize)]
struct RegistrationRequest<'a> {
    name: &'a str,
    edge_id: &'a str,
    e2e_public_key: &'a str,
    e2e_key_version: u64,
    e2e_key_proof: &'a str,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct E2eConfig {
    registration_proof_public_key: String,
    capability_verification_key: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct StoredIdentity {
    device_id: String,
    key_version: u64,
    private_key: String,
    capability_verification_key: String,
}

struct RegistrationApi<'a> {
    client: &'a Client,
    origin: &'a Url,
    credential: &'a str,
}

pub async fn enable(vars: &HashMap<String, String>) -> Result<RemoteAccess, String> {
    let home = login::pontia_home(vars)?;
    let origin = login::auth_origin(vars)?;
    let credential = read_credential(&home.join("auth.json"), "enabling")?;
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
    let e2e_config = api.e2e_config().await?;

    if let Some(device) = api.find_device(&device_id).await? {
        let identity_path = home.join("e2e-identity.json");
        if stored_identity_matches(&identity_path, &device)? {
            print_registered(&device, true);
            return remote_access(device);
        }
        let device = rotate_registered_identity(&api, &identity_path, device, &e2e_config).await?;
        print_registered(&device, true);
        return remote_access(device);
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

    let pending = generate_identity(
        &device_id,
        1,
        e2e_config.capability_verification_key.clone(),
    )?;
    let public_key = identity_public_key(&pending)?;
    let proof = identity_proof(&pending, &e2e_config.registration_proof_public_key)?;
    let device = api
        .register_device(&device_id, &name, &edge.id, &public_key, 1, &proof)
        .await?;
    let stored = StoredIdentity {
        capability_verification_key: device.capability_verification_key.clone(),
        ..pending
    };
    write_identity(&home.join("e2e-identity.json"), &stored)?;
    print_registered(&device, false);
    remote_access(device)
}

pub async fn rotate_key(vars: &HashMap<String, String>) -> Result<(), String> {
    let home = login::pontia_home(vars)?;
    let device_id = read_device_id(&home.join("config.toml"))?
        .ok_or_else(|| "remote access is disabled; run `pontia remote enable` first".to_string())?;
    let origin = login::auth_origin(vars)?;
    let credential = read_credential(&home.join("auth.json"), "rotating the remote identity key")?;
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
    let device = api.find_device(&device_id).await?.ok_or_else(|| {
        "the configured remote device is not registered; run `pontia remote enable`".to_string()
    })?;
    let e2e_config = api.e2e_config().await?;
    let device =
        rotate_registered_identity(&api, &home.join("e2e-identity.json"), device, &e2e_config)
            .await?;
    println!(
        "Device identity key rotated to version {}.",
        device.e2e_key_version
    );
    Ok(())
}

fn generate_identity(
    device_id: &str,
    key_version: u64,
    trust_key: String,
) -> Result<StoredIdentity, String> {
    let id = Uuid::parse_str(device_id).map_err(|_| "invalid device ID".to_string())?;
    let identity = DeviceIdentity::generate(*id.as_bytes(), key_version)
        .map_err(|error| format!("failed to generate device identity: {error}"))?;
    Ok(StoredIdentity {
        device_id: device_id.to_string(),
        key_version,
        private_key: URL_SAFE_NO_PAD.encode(identity.private_bytes()),
        capability_verification_key: trust_key,
    })
}

fn identity_public_key(stored: &StoredIdentity) -> Result<String, String> {
    let id = Uuid::parse_str(&stored.device_id)
        .map_err(|_| "invalid stored device identity".to_string())?;
    let private = URL_SAFE_NO_PAD
        .decode(&stored.private_key)
        .map_err(|_| "invalid stored device identity".to_string())?;
    let identity = DeviceIdentity::from_private_bytes(*id.as_bytes(), stored.key_version, &private)
        .map_err(|_| "invalid stored device identity".to_string())?;
    Ok(URL_SAFE_NO_PAD.encode(identity.public_key()))
}

fn identity_proof(identity: &StoredIdentity, cloud_public_key: &str) -> Result<String, String> {
    let private: [u8; 32] = URL_SAFE_NO_PAD
        .decode(&identity.private_key)
        .ok()
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(|| "invalid stored device identity".to_string())?;
    let cloud_public: [u8; 32] = URL_SAFE_NO_PAD
        .decode(cloud_public_key)
        .ok()
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(|| "Cloud returned an invalid registration proof key".to_string())?;
    let public = identity_public_key(identity)?;
    let public_bytes = URL_SAFE_NO_PAD.decode(public).unwrap();
    let shared = StaticSecret::from(private).diffie_hellman(&X25519PublicKey::from(cloud_public));
    let mut key = [0_u8; 32];
    Hkdf::<Sha256>::new(None, shared.as_bytes())
        .expand(b"pontia-device-key-registration-v1\0", &mut key)
        .map_err(|_| "failed to derive device key proof".to_string())?;
    let mut message = identity.device_id.as_bytes().to_vec();
    message.push(0);
    message.extend_from_slice(&identity.key_version.to_be_bytes());
    message.extend_from_slice(&public_bytes);
    let mut mac = Hmac::<Sha256>::new_from_slice(&key).unwrap();
    mac.update(&message);
    Ok(URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes()))
}

fn read_identity(path: &Path) -> Result<Option<StoredIdentity>, String> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("failed to read {}: {error}", path.display())),
    };
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|_| format!("invalid device identity in {}", path.display()))
}

fn write_identity(path: &Path, identity: &StoredIdentity) -> Result<(), String> {
    let mut bytes = serde_json::to_vec_pretty(identity)
        .map_err(|error| format!("failed to serialize device identity: {error}"))?;
    bytes.push(b'\n');
    private_file::atomic_write(path, &bytes)
}

fn stored_identity_matches(path: &Path, device: &Device) -> Result<bool, String> {
    let Some(identity) = read_identity(path)? else {
        return Ok(false);
    };
    Ok(identity.device_id == device.id
        && identity.key_version == device.e2e_key_version
        && identity.capability_verification_key == device.capability_verification_key
        && identity_public_key(&identity)? == device.e2e_public_key)
}

async fn rotate_registered_identity(
    api: &RegistrationApi<'_>,
    path: &Path,
    device: Device,
    config: &E2eConfig,
) -> Result<Device, String> {
    let key_version = device
        .e2e_key_version
        .checked_add(1)
        .ok_or_else(|| "device identity key version is exhausted".to_string())?;
    let pending = generate_identity(
        &device.id,
        key_version,
        config.capability_verification_key.clone(),
    )?;
    let public_key = identity_public_key(&pending)?;
    let proof = identity_proof(&pending, &config.registration_proof_public_key)?;
    write_identity(path, &pending)?;
    api.register_device(
        &device.id,
        device.name.as_deref().unwrap_or("unnamed device"),
        &device.edge_id,
        &public_key,
        key_version,
        &proof,
    )
    .await
}

pub async fn disable(vars: &HashMap<String, String>) -> Result<(), String> {
    let home = login::pontia_home(vars)?;
    let config_path = home.join("config.toml");
    let Some(device_id) = read_device_id(&config_path)? else {
        println!("Remote access is already disabled.");
        return Ok(());
    };
    let origin = login::auth_origin(vars)?;
    let credential = read_credential(&home.join("auth.json"), "disabling")?;
    let client = Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(|error| {
            format!("failed to create the remote registration HTTP client: {error}")
        })?;
    RegistrationApi {
        client: &client,
        origin: &origin,
        credential: &credential,
    }
    .unregister_device(&device_id)
    .await?;
    remove_device_id(&config_path, &device_id)?;
    match fs::remove_file(home.join("e2e-identity.json")) {
        Ok(()) => {}
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => {
            return Err(format!(
                "remote access was disabled, but the local identity could not be removed: {error}"
            ));
        }
    }
    println!("Remote access disabled.");
    Ok(())
}

fn read_credential(path: &Path, action: &str) -> Result<String, String> {
    let contents = fs::read(path).map_err(|error| {
        if error.kind() == ErrorKind::NotFound {
            format!("not logged in; run `pontia login` before {action} remote access")
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

fn read_document(path: &Path) -> Result<DocumentMut, String> {
    let original = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == ErrorKind::NotFound => String::new(),
        Err(error) => return Err(format!("failed to read {}: {error}", path.display())),
    };
    if original.trim().is_empty() {
        Ok(DocumentMut::new())
    } else {
        original
            .parse::<DocumentMut>()
            .map_err(|error| format!("failed to parse {}: {error}", path.display()))
    }
}

fn device_id(document: &DocumentMut, path: &Path) -> Result<Option<String>, String> {
    let item = document
        .get("remote")
        .and_then(Item::as_table_like)
        .and_then(|remote| remote.get("device_id"));
    let Some(item) = item else {
        return Ok(None);
    };
    let Some(value) = item.as_str() else {
        return Err(format!(
            "remote.device_id in {} must be a UUIDv7 string",
            path.display()
        ));
    };
    if !uuid_v7(value) {
        return Err(format!(
            "remote.device_id in {} must be a UUIDv7",
            path.display()
        ));
    }
    Ok(Some(value.to_string()))
}

fn read_device_id(path: &Path) -> Result<Option<String>, String> {
    device_id(&read_document(path)?, path)
}

fn ensure_device_id(path: &Path) -> Result<String, String> {
    let mut document = read_document(path)?;
    if let Some(device_id) = device_id(&document, path)? {
        return Ok(device_id);
    }

    let device_id = Uuid::now_v7().to_string();
    document["remote"]["device_id"] = toml_edit::value(&device_id);
    private_file::atomic_write(path, document.to_string().as_bytes())?;
    Ok(device_id)
}

fn remove_device_id(path: &Path, expected_device_id: &str) -> Result<(), String> {
    let mut document = read_document(path)?;
    match device_id(&document, path)? {
        None => return Ok(()),
        Some(device_id) if device_id != expected_device_id => {
            return Err(format!(
                "remote.device_id in {} changed while remote access was being disabled; the new value was preserved",
                path.display()
            ));
        }
        Some(_) => {}
    }
    let remote_is_empty = {
        let remote = document
            .get_mut("remote")
            .and_then(Item::as_table_like_mut)
            .expect("validated remote table");
        remote.remove("device_id");
        remote.is_empty()
    };
    if remote_is_empty {
        document.remove("remote");
    }
    private_file::atomic_write(path, document.to_string().as_bytes())
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
    async fn e2e_config(&self) -> Result<E2eConfig, String> {
        let response = self
            .client
            .get(endpoint(self.origin, "api/remote/e2e-config")?)
            .bearer_auth(self.credential)
            .send()
            .await
            .map_err(|error| uncertain("fetch E2E configuration", error))?;
        if response.status() == StatusCode::UNAUTHORIZED {
            return Err(login_required());
        }
        if !response.status().is_success() {
            return Err(format!(
                "the cloud rejected the E2E configuration request ({})",
                response.status()
            ));
        }
        let config: E2eConfig = response
            .json()
            .await
            .map_err(|_| "the cloud returned invalid E2E configuration".to_string())?;
        if !valid_key(&config.registration_proof_public_key)
            || !valid_key(&config.capability_verification_key)
        {
            return Err("the cloud returned invalid E2E configuration".to_string());
        }
        Ok(config)
    }

    async fn unregister_device(&self, device_id: &str) -> Result<(), String> {
        let response = self
            .client
            .delete(endpoint(
                self.origin,
                &format!("api/remote/devices/{device_id}"),
            )?)
            .bearer_auth(self.credential)
            .send()
            .await
            .map_err(|error| uncertain("unregister the device", error))?;
        match response.status() {
            StatusCode::NO_CONTENT | StatusCode::NOT_FOUND => Ok(()),
            StatusCode::UNAUTHORIZED => Err(login_required()),
            StatusCode::CONFLICT => Err(
                "the configured device ID is registered to a different user; refusing to remove it"
                    .to_string(),
            ),
            status => Err(format!(
                "the cloud rejected device unregistration ({status}); the configured device ID was preserved"
            )),
        }
    }

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
                "the cloud rejected the device registration query ({status}); the configured device ID was preserved"
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
                "the cloud rejected the official edge request ({})",
                response.status()
            ));
        }
        let edges: Vec<Edge> = response
            .json()
            .await
            .map_err(|_| "the cloud returned an invalid official edge list".to_string())?;
        if edges.is_empty() {
            return Err("no official edge is currently available for registration".to_string());
        }
        if edges
            .iter()
            .any(|edge| !uuid_v7(&edge.id) || edge.name.is_empty())
        {
            return Err("the cloud returned an invalid official edge list".to_string());
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
        e2e_public_key: &str,
        e2e_key_version: u64,
        e2e_key_proof: &str,
    ) -> Result<Device, String> {
        let response = self
            .client
            .put(endpoint(
                self.origin,
                &format!("api/remote/devices/{device_id}"),
            )?)
            .bearer_auth(self.credential)
            .json(&RegistrationRequest {
                name,
                edge_id,
                e2e_public_key,
                e2e_key_version,
                e2e_key_proof,
            })
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
                Err("the cloud rejected the device registration data".to_string())
            }
            status => Err(format!(
                "the cloud rejected device registration ({status}); the configured device ID was preserved"
            )),
        }
    }
}

async fn parse_device(response: reqwest::Response, expected_id: &str) -> Result<Device, String> {
    let device: Device = response.json().await.map_err(|_| uncertain_result())?;
    if device.id != expected_id
        || !uuid_v7(&device.id)
        || !uuid_v7(&device.edge_id)
        || !valid_device_handle(&device.handle)
        || (device.e2e_key_version == 0 && !device.e2e_public_key.is_empty())
        || (device.e2e_key_version > 0 && !valid_key(&device.e2e_public_key))
        || !valid_key(&device.capability_verification_key)
    {
        return Err(uncertain_result());
    }
    Ok(device)
}

fn valid_key(value: &str) -> bool {
    URL_SAFE_NO_PAD
        .decode(value)
        .is_ok_and(|bytes| bytes.len() == 32 && URL_SAFE_NO_PAD.encode(bytes) == value)
}

fn valid_device_handle(value: &str) -> bool {
    (4..=48).contains(&value.len())
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"_-".contains(&byte))
}

fn remote_access(device: Device) -> Result<RemoteAccess, String> {
    if !valid_device_handle(&device.handle) {
        return Err(uncertain_result());
    }
    Ok(RemoteAccess {
        device_handle: device.handle,
    })
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
    "the cloud returned an invalid device registration result; the registration state is unknown and the configured device ID was preserved".to_string()
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

    // Exercise remote configuration directly: successful CLI commands also manage
    // the user's real service, which a temporary PONTIA_HOME does not isolate.
    #[tokio::test]
    async fn disabling_unconfigured_remote_access_does_not_create_configuration() {
        let root = tempdir().unwrap();
        let vars = HashMap::from([("PONTIA_HOME".to_string(), root.path().display().to_string())]);

        disable(&vars).await.unwrap();
        disable(&vars).await.unwrap();

        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
    }

    #[tokio::test]
    async fn disabling_unconfigured_remote_access_preserves_existing_configuration() {
        let root = tempdir().unwrap();
        let path = root.path().join("config.toml");
        let original = "# existing settings\nbind_addr = '127.0.0.1:9000'\n";
        fs::write(&path, original).unwrap();
        let vars = HashMap::from([("PONTIA_HOME".to_string(), root.path().display().to_string())]);

        disable(&vars).await.unwrap();
        disable(&vars).await.unwrap();

        assert_eq!(fs::read_to_string(path).unwrap(), original);
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
    }

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

    #[test]
    fn removing_device_id_preserves_other_configuration() {
        let root = tempdir().unwrap();
        let path = root.path().join("config.toml");
        let device_id = "0195e7c1-1b22-7c33-9d44-123456789abc";
        fs::write(
            &path,
            format!(
                "bind_addr = '127.0.0.1:9000'\n[remote]\ndevice_id = '{device_id}'\nfuture = true\n"
            ),
        )
        .unwrap();

        remove_device_id(&path, device_id).unwrap();

        assert_eq!(
            fs::read_to_string(path).unwrap(),
            "bind_addr = '127.0.0.1:9000'\n[remote]\nfuture = true\n"
        );
    }

    #[test]
    fn removing_the_only_remote_setting_removes_the_table() {
        let root = tempdir().unwrap();
        let path = root.path().join("config.toml");
        let device_id = "0195e7c1-1b22-7c33-9d44-123456789abc";
        fs::write(&path, format!("[remote]\ndevice_id = '{device_id}'\n")).unwrap();

        remove_device_id(&path, device_id).unwrap();

        assert_eq!(fs::read_to_string(path).unwrap(), "");
    }

    #[test]
    fn validates_device_handles_used_in_remote_dashboard_urls() {
        assert!(valid_device_handle("office-mac"));
        assert!(valid_device_handle("home_2"));
        assert!(!valid_device_handle("abc"));
        assert!(!valid_device_handle("Office-Mac"));
        assert!(!valid_device_handle("office/mac"));
    }
}
