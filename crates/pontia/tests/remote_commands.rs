// Exercise the remote command boundary with a loopback Cloud transport. Production
// auth-origin validation requires HTTPS and is covered by pontia-config tests.
#[allow(dead_code)]
#[path = "../src/login.rs"]
mod real_login;
#[allow(dead_code)]
#[path = "../src/remote.rs"]
mod remote;

mod login {
    pub(super) use super::real_login::{pontia_home, validate_token};

    pub(super) fn auth_origin(
        vars: &std::collections::HashMap<String, String>,
    ) -> Result<url::Url, String> {
        let origin = vars
            .get("PONTIA_AUTH_ORIGIN")
            .expect("explicit test origin");
        let url = url::Url::parse(origin).unwrap();
        assert_eq!(url.host_str(), Some("127.0.0.1"));
        Ok(url)
    }
}

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use clap::{Parser, Subcommand};
use remote::{ServiceRestart, run};
use x25519_dalek::{PublicKey as X25519PublicKey, StaticSecret};

#[derive(Debug, Parser)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Remote(remote::RemoteCommand),
    Init,
}
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    fs,
    io::{ErrorKind, Read, Write},
    net::TcpListener,
    sync::{Arc, Mutex, mpsc},
    thread,
    time::Duration,
};
use tempfile::TempDir;

const FIRST_EDGE: &str = "0195e7c1-1b22-7c33-9d44-123456789abc";
const SECOND_EDGE: &str = "0195e7c2-1b22-7c33-9d44-123456789abc";
const TOKEN: &str =
    "ptr_v1_0199791c-6600-7000-8000-000000000001_AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

#[derive(Default)]
struct CloudState {
    device: Option<Value>,
    registrations: Vec<Value>,
    unauthorized: bool,
    edges: Option<Value>,
}

struct Cloud {
    root: TempDir,
    vars: HashMap<String, String>,
    state: Arc<Mutex<CloudState>>,
    stop: mpsc::Sender<()>,
    server: Option<thread::JoinHandle<()>>,
}

impl Cloud {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        fs::write(
            root.path().join("auth.json"),
            json!({"token": TOKEN}).to_string(),
        )
        .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let vars = HashMap::from([
            ("PONTIA_HOME".into(), root.path().display().to_string()),
            (
                "PONTIA_AUTH_ORIGIN".into(),
                format!("http://{}", listener.local_addr().unwrap()),
            ),
        ]);
        let state = Arc::new(Mutex::new(CloudState::default()));
        let server_state = state.clone();
        let (stop, stopped) = mpsc::channel();
        let server = thread::spawn(move || {
            while stopped.try_recv().is_err() {
                let (mut stream, _) = match listener.accept() {
                    Ok(connection) => connection,
                    Err(error) if error.kind() == ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    Err(error) => panic!("accept request: {error}"),
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                stream
                    .set_write_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut bytes = Vec::new();
                let (method, path, body) = loop {
                    let mut buffer = [0; 4096];
                    let count = stream.read(&mut buffer).unwrap();
                    assert_ne!(count, 0);
                    bytes.extend_from_slice(&buffer[..count]);
                    let text = String::from_utf8_lossy(&bytes);
                    let Some((headers, body)) = text.split_once("\r\n\r\n") else {
                        continue;
                    };
                    let length = headers
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length: ")
                                .map(|length| length.parse::<usize>().unwrap())
                        })
                        .unwrap_or(0);
                    if body.len() < length {
                        continue;
                    }
                    assert!(headers.to_ascii_lowercase().contains(&format!(
                        "authorization: bearer {}",
                        TOKEN.to_ascii_lowercase()
                    )));
                    let mut request = headers.lines().next().unwrap().split_whitespace();
                    break (
                        request.next().unwrap().to_string(),
                        request.next().unwrap().to_string(),
                        if length == 0 {
                            Value::Null
                        } else {
                            serde_json::from_str(body).unwrap()
                        },
                    );
                };
                let mut state = server_state.lock().unwrap();
                let trust_key = URL_SAFE_NO_PAD.encode([3_u8; 32]);
                let (status, response) = if state.unauthorized {
                    (401, Value::Null)
                } else if method == "GET" && path == "/api/remote/e2e-config" {
                    (
                        200,
                        json!({
                            "registration_proof_public_key": URL_SAFE_NO_PAD.encode(
                                X25519PublicKey::from(&StaticSecret::from([7_u8; 32])).as_bytes()
                            ),
                            "capability_verification_key": trust_key,
                        }),
                    )
                } else if method == "GET" && path == "/api/remote/edges" {
                    (
                        200,
                        state.edges.clone().unwrap_or_else(|| {
                            json!([
                                {"id": FIRST_EDGE, "name": "First edge"},
                                {"id": SECOND_EDGE, "name": "Second edge"},
                            ])
                        }),
                    )
                } else if let Some(id) = path.strip_prefix("/api/remote/devices/") {
                    match method.as_str() {
                        "GET" => match &state.device {
                            Some(device) => (200, device.clone()),
                            None => (404, Value::Null),
                        },
                        "PUT" => {
                            let device = json!({
                                "id": id,
                                "device_handle": "test-device",
                                "name": body["name"],
                                "edge_id": body["edge_id"],
                                "edge_name": "Selected edge",
                                "e2e_public_key": body["e2e_public_key"],
                                "e2e_key_version": body["e2e_key_version"],
                                "capability_verification_key": trust_key,
                            });
                            state.registrations.push(body);
                            state.device = Some(device.clone());
                            (201, device)
                        }
                        _ => panic!("unexpected mutation: {method} {path}"),
                    }
                } else {
                    panic!("unexpected request: {method} {path}");
                };
                let body = response.to_string();
                write!(stream, "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            }
        });
        Self {
            root,
            vars,
            state,
            stop,
            server: Some(server),
        }
    }

    async fn run(&self, args: &[&str]) -> Result<ServiceRestart, String> {
        let cli = crate::Cli::try_parse_from(
            ["pontia", "remote"].into_iter().chain(args.iter().copied()),
        )
        .unwrap();
        let crate::Command::Remote(command) = cli.command else {
            panic!("not a remote command")
        };
        // Do not call main's dispatcher: it can restart the real user's service.
        run(command, &self.vars).await
    }
}

impl Drop for Cloud {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        let result = self.server.take().unwrap().join();
        if !thread::panicking() {
            result.unwrap();
        }
    }
}

#[test]
fn enable_requires_both_registration_parameters() {
    for args in [
        vec!["pontia", "remote", "enable", "--edge", SECOND_EDGE],
        vec!["pontia", "remote", "enable", "--name", "Agent device"],
    ] {
        let error = crate::Cli::try_parse_from(args).unwrap_err();
        assert_eq!(
            error.kind(),
            clap::error::ErrorKind::MissingRequiredArgument
        );
    }
    assert!(crate::Cli::try_parse_from(["pontia", "remote", "enable"]).is_ok());
}

#[tokio::test]
async fn parameterized_registration_selects_the_requested_edge_and_persists_identity() {
    let cloud = Cloud::new();
    let result = cloud
        .run(&["enable", "--edge", SECOND_EDGE, "--name", "Agent device"])
        .await
        .unwrap();
    assert!(matches!(result, ServiceRestart::Required));
    let state = cloud.state.lock().unwrap();
    assert_eq!(state.registrations.len(), 1);
    let registration = &state.registrations[0];
    assert_eq!(registration["edge_id"], SECOND_EDGE);
    assert_eq!(registration["name"], "Agent device");
    assert_eq!(registration["e2e_key_version"], 1);
    assert_eq!(
        URL_SAFE_NO_PAD
            .decode(registration["e2e_key_proof"].as_str().unwrap())
            .unwrap()
            .len(),
        32
    );
    let config: toml::Value =
        toml::from_str(&fs::read_to_string(cloud.root.path().join("config.toml")).unwrap())
            .unwrap();
    let identity: Value =
        serde_json::from_slice(&fs::read(cloud.root.path().join("e2e-identity.json")).unwrap())
            .unwrap();
    assert_eq!(
        identity["device_id"].as_str(),
        config["remote"]["device_id"].as_str()
    );
    assert_eq!(identity["device_id"], state.device.as_ref().unwrap()["id"]);
    assert_eq!(identity["key_version"], 1);
}

#[tokio::test]
async fn enabling_an_existing_device_preserves_its_registration_and_identity() {
    let cloud = Cloud::new();
    cloud
        .run(&["enable", "--edge", SECOND_EDGE, "--name", "Original device"])
        .await
        .unwrap();
    let path = cloud.root.path().join("e2e-identity.json");
    let identity = fs::read(&path).unwrap();
    cloud
        .run(&[
            "enable",
            "--edge",
            FIRST_EDGE,
            "--name",
            "Replacement device",
        ])
        .await
        .unwrap();
    assert_eq!(fs::read(path).unwrap(), identity);
    let state = cloud.state.lock().unwrap();
    assert_eq!(state.registrations.len(), 1);
    let device = state.device.as_ref().unwrap();
    assert_eq!(device["name"], "Original device");
    assert_eq!(device["edge_id"], SECOND_EDGE);
}

#[tokio::test]
async fn unavailable_edge_and_invalid_name_do_not_register_a_device() {
    for (edge, name) in [
        ("0195e7c3-1b22-7c33-9d44-123456789abc", "Agent device"),
        (SECOND_EDGE, "   "),
    ] {
        let cloud = Cloud::new();
        assert!(
            cloud
                .run(&["enable", "--edge", edge, "--name", name])
                .await
                .is_err()
        );
        assert!(cloud.state.lock().unwrap().registrations.is_empty());
        assert!(!cloud.root.path().join("e2e-identity.json").exists());
    }
}

#[tokio::test]
async fn listing_edges_does_not_change_configuration_or_require_a_service_restart() {
    let cloud = Cloud::new();
    assert!(matches!(
        cloud.run(&["edges"]).await.unwrap(),
        ServiceRestart::NotRequired
    ));
    assert!(cloud.state.lock().unwrap().registrations.is_empty());
    assert_eq!(fs::read_dir(cloud.root.path()).unwrap().count(), 1);
}

#[tokio::test]
async fn an_empty_edge_list_can_be_listed_but_cannot_register_a_device() {
    let cloud = Cloud::new();
    cloud.state.lock().unwrap().edges = Some(json!([]));
    assert!(matches!(
        cloud.run(&["edges"]).await.unwrap(),
        ServiceRestart::NotRequired
    ));
    assert!(
        cloud
            .run(&["enable", "--edge", SECOND_EDGE, "--name", "Agent device"])
            .await
            .is_err()
    );
    assert!(cloud.state.lock().unwrap().registrations.is_empty());
}

#[tokio::test]
async fn invalid_cloud_edges_are_rejected() {
    let cloud = Cloud::new();
    cloud.state.lock().unwrap().edges = Some(json!([{"id": "invalid", "name": "Invalid edge"}]));
    assert!(cloud.run(&["edges"]).await.is_err());
    assert_eq!(fs::read_dir(cloud.root.path()).unwrap().count(), 1);
}

#[tokio::test]
async fn expired_login_does_not_register_a_device() {
    let cloud = Cloud::new();
    cloud.state.lock().unwrap().unauthorized = true;
    assert!(cloud.run(&["edges"]).await.is_err());
    assert!(
        cloud
            .run(&["enable", "--edge", SECOND_EDGE, "--name", "Agent device"])
            .await
            .is_err()
    );
    assert!(cloud.state.lock().unwrap().registrations.is_empty());
    assert!(!cloud.root.path().join("e2e-identity.json").exists());
}

#[tokio::test]
async fn missing_login_rejects_noninteractive_commands_without_creating_configuration() {
    let root = tempfile::tempdir().unwrap();
    let vars = HashMap::from([
        ("PONTIA_HOME".into(), root.path().display().to_string()),
        ("PONTIA_AUTH_ORIGIN".into(), "http://127.0.0.1:1".into()),
    ]);
    for args in [
        vec!["pontia", "remote", "edges"],
        vec![
            "pontia",
            "remote",
            "enable",
            "--edge",
            SECOND_EDGE,
            "--name",
            "Agent device",
        ],
    ] {
        let crate::Command::Remote(command) = crate::Cli::try_parse_from(args).unwrap().command
        else {
            panic!("not a remote command")
        };
        assert!(run(command, &vars).await.is_err());
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
    }
}
