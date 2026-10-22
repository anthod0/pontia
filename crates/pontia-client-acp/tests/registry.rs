use pontia_client_acp::AcpAgentConfig;
use pontia_client_acp::registry::Registry;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};

struct Server {
    url: String,
    stop: Arc<AtomicBool>,
    task: Option<thread::JoinHandle<()>>,
}
impl Server {
    fn new(document: impl FnOnce(&str) -> Value, archive: Vec<u8>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let document = document(&url).to_string().into_bytes();
        let stop = Arc::new(AtomicBool::new(false));
        let signal = stop.clone();
        let task = thread::spawn(move || {
            for stream in listener.incoming() {
                if signal.load(Ordering::Acquire) {
                    break;
                }
                let mut stream = stream.unwrap();
                let mut buffer = [0; 8192];
                let count = stream.read(&mut buffer).unwrap();
                let request = String::from_utf8_lossy(&buffer[..count]);
                let body = if request.starts_with("GET /registry ") {
                    &document
                } else {
                    &archive
                };
                write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).unwrap();
                stream.write_all(body).unwrap();
            }
        });
        Self {
            url,
            stop,
            task: Some(task),
        }
    }
    fn registry(&self) -> Registry {
        Registry::new(format!("{}/registry", self.url))
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let _ = TcpStream::connect(self.url.trim_start_matches("http://"));
        self.task.take().unwrap().join().unwrap();
    }
}
fn config(registry: &str) -> AcpAgentConfig {
    AcpAgentConfig {
        name: None,
        registry_id: Some(registry.into()),
        command: String::new(),
        args: vec![],
        env: BTreeMap::new(),
    }
}
fn platform() -> String {
    format!(
        "{}-{}",
        if cfg!(target_os = "macos") {
            "darwin"
        } else {
            "linux"
        },
        std::env::consts::ARCH
    )
}
fn archive() -> Vec<u8> {
    let mut tar = tar::Builder::new(Vec::new());
    let script = b"#!/bin/sh\nprintf '%s\\n' \"$REGISTRY_ENV\" \"$1\"\n";
    let mut header = tar::Header::new_gnu();
    header.set_size(script.len() as u64);
    header.set_mode(0o755);
    header.set_cksum();
    tar.append_data(&mut header, "peer", &script[..]).unwrap();
    tar.into_inner().unwrap()
}

#[tokio::test]
async fn binary_launch_uses_registry_arguments_config_environment_and_an_atomic_cached_install() {
    let root = tempfile::tempdir().unwrap();
    let archive = archive();
    let sha256 = format!("{:x}", Sha256::digest(&archive));
    let server = Server::new(
        |url| {
            json!({"agents":[{"id":"registry-id","name":"Registry Agent","distribution":{
                "binary":{platform():{"archive":format!("{url}/peer.tar"),"cmd":"./peer","args":["${LITERAL} ; *"],"sha256":sha256,"env":{"REGISTRY_ENV":"default"}}},
                "npx":{"package":"must-not-fallback"}
            }}]})
        },
        archive,
    );
    let registry = server.registry();
    let mut selected = config("registry-id");
    selected
        .env
        .insert("REGISTRY_ENV".into(), "override".into());
    assert_eq!(
        registry.name("registry-id").await.unwrap(),
        "Registry Agent"
    );
    let launch = registry
        .resolve(&selected, &root.path().join("cache"), root.path())
        .await
        .unwrap();
    assert_eq!(launch.args, ["${LITERAL} ; *"]);
    assert_eq!(launch.registry_id, None);
    let output = tokio::process::Command::new(&launch.command)
        .args(&launch.args)
        .envs(&launch.env)
        .current_dir(root.path())
        .output()
        .await
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"override\n${LITERAL} ; *\n");
    drop(server);
    let cached = registry
        .resolve(&selected, &root.path().join("cache"), root.path())
        .await
        .unwrap();
    assert_eq!(launch.command, cached.command);
    let output = tokio::process::Command::new(&cached.command)
        .args(&cached.args)
        .envs(&cached.env)
        .current_dir(root.path())
        .output()
        .await
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"override\n${LITERAL} ; *\n");
}

#[tokio::test]
async fn package_runners_preserve_registry_arguments_and_never_expand_environment_values() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    for command in ["npx", "uvx"] {
        let path = root.path().join(command);
        std::fs::write(&path, "#!/bin/sh\nexit 0\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let server = Server::new(
        |_| {
            json!({"agents":[
                {"id":"node","name":"Node Agent","distribution":{"npx":{"package":"@scope/agent@1.2.3","args":["--acp","argument with spaces"],"env":{"TOKEN":"${UNEXPANDED}"}},"uvx":{"package":"not-selected"}}},
                {"id":"python","name":"Python Agent","distribution":{"uvx":{"package":"python-agent==2.3","args":["--acp"]}}}
            ]})
        },
        vec![],
    );
    let registry = server.registry();
    for (id, runner, expected) in [
        (
            "node",
            "npx",
            vec![
                "--yes",
                "--",
                "@scope/agent@1.2.3",
                "--acp",
                "argument with spaces",
            ],
        ),
        ("python", "uvx", vec!["--", "python-agent==2.3", "--acp"]),
    ] {
        let mut selected = config(id);
        selected
            .env
            .insert("PATH".into(), root.path().display().to_string());
        let launch = registry
            .resolve(&selected, &root.path().join("cache"), root.path())
            .await
            .unwrap();
        assert_eq!(launch.command, root.path().join(runner).to_str().unwrap());
        assert_eq!(launch.args, expected);
        if id == "node" {
            assert_eq!(launch.env["TOKEN"], "${UNEXPANDED}");
        }
    }
}

#[tokio::test]
async fn invalid_checksums_commands_and_missing_distributions_fail_without_publishing_an_install() {
    let root = tempfile::tempdir().unwrap();
    let server = Server::new(
        |url| {
            json!({"agents":[
                {"id":"checksum","name":"Bad checksum","distribution":{"binary":{platform():{"archive":format!("{url}/peer.tar"),"cmd":"peer","sha256":"0000"}},"npx":{"package":"must-not-fallback"}}},
                {"id":"escape","name":"Escaping command","distribution":{"binary":{platform():{"archive":format!("{url}/peer.tar"),"cmd":"../outside"}}}},
                {"id":"unsupported","name":"Unsupported","distribution":{}}
            ]})
        },
        archive(),
    );
    let registry = server.registry();
    for id in ["checksum", "escape", "unsupported", "missing"] {
        assert!(
            registry
                .resolve(&config(id), &root.path().join("cache"), root.path())
                .await
                .is_err()
        );
    }
    assert!(!root.path().join("cache").exists());
    assert!(!root.path().join("outside").exists());
}

#[tokio::test]
async fn zip_distributions_are_extracted_and_archive_links_are_rejected() {
    let root = tempfile::tempdir().unwrap();
    for symlink in [false, true] {
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default();
        if symlink {
            writer.add_symlink("peer", "../outside", options).unwrap();
        } else {
            writer.start_file("peer", options).unwrap();
            writer.write_all(b"#!/bin/sh\nexit 0\n").unwrap();
        }
        let archive = writer.finish().unwrap().into_inner();
        let server = Server::new(
            |url| json!({"agents":[{"id":"zip","name":"Zip Agent","distribution":{"binary":{platform():{"archive":format!("{url}/peer.zip"),"cmd":"peer"}}}}]}),
            archive,
        );
        let cache = root.path().join(if symlink { "links" } else { "valid" });
        let result = server
            .registry()
            .resolve(&config("zip"), &cache, root.path())
            .await;
        if symlink {
            assert!(result.is_err());
            assert!(!root.path().join("outside").exists());
        } else {
            assert!(std::path::Path::new(&result.unwrap().command).is_file());
        }
    }
}

#[tokio::test]
async fn compressed_tar_distributions_launch_the_declared_peer() {
    let root = tempfile::tempdir().unwrap();
    let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gzip.write_all(&archive()).unwrap();
    let mut bzip = bzip2::write::BzEncoder::new(Vec::new(), bzip2::Compression::default());
    bzip.write_all(&archive()).unwrap();
    for (extension, bytes) in [
        ("tar.gz", gzip.finish().unwrap()),
        ("tar.bz2", bzip.finish().unwrap()),
    ] {
        let server = Server::new(
            |url| json!({"agents":[{"id":"compressed","name":"Compressed","distribution":{"binary":{platform():{"archive":format!("{url}/peer.{extension}"),"cmd":"peer"}}}}]}),
            bytes,
        );
        let launch = server
            .registry()
            .resolve(
                &config("compressed"),
                &root.path().join(extension),
                root.path(),
            )
            .await
            .unwrap();
        let output = tokio::process::Command::new(launch.command)
            .env("REGISTRY_ENV", "unpacked")
            .arg("literal")
            .output()
            .await
            .unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout, b"unpacked\nliteral\n");
    }
}

#[tokio::test]
async fn binary_recovery_uses_the_state_root_and_offline_registry_names_do_not_block_inspection() {
    use pontia_application::{
        AppState, CreateSessionRequest, client_contract::ClientServicePhase,
        clients::ClientRegistry,
    };
    use pontia_client_acp::AcpConfig;
    use pontia_storage_sqlite::{connect_sqlite, run_migrations};
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    let mut tar = tar::Builder::new(Vec::new());
    let script = format!(
        "#!/usr/bin/env python3\n{}",
        include_str!("fixtures/agent.py")
    );
    let mut header = tar::Header::new_gnu();
    header.set_size(script.len() as u64);
    header.set_mode(0o755);
    header.set_cksum();
    tar.append_data(&mut header, "peer", script.as_bytes())
        .unwrap();
    let server = Server::new(
        |url| json!({"agents":[{"id":"native-registry-id","name":"Registry Name","distribution":{"binary":{platform():{"archive":format!("{url}/peer.tar"),"cmd":"peer"}}}}]}),
        tar.into_inner().unwrap(),
    );
    let mut selected = config("native-registry-id");
    selected
        .env
        .insert("ACP_FIXTURE_ROOT".into(), root.path().display().to_string());
    let config = AcpConfig {
        agents: BTreeMap::from([("company-choice".into(), selected)]),
    };
    let pool = connect_sqlite("sqlite::memory:").await.unwrap();
    run_migrations(&pool).await.unwrap();
    let state_root = root.path().join("state");
    let mut clients = ClientRegistry::default();
    clients.register(pontia_client_acp::registration_with_registry(
        config.clone(),
        server.registry(),
    ));
    let app = AppState::builder(pool.clone(), state_root.clone())
        .clients(clients.clone())
        .build();
    let tasks = clients
        .start_services(app.clone(), ClientServicePhase::Transport)
        .await
        .unwrap();
    let request: CreateSessionRequest = serde_json::from_value(json!({"client_type":"acp","workspace":workspace,"metadata":{"acp_agent_id":"company-choice"}})).unwrap();
    let result = app
        .session_commands()
        .create_session(request)
        .await
        .unwrap();
    let session = result.session_id().unwrap().to_string();
    let binding = pontia_application::AgentBindingService::new(pool.clone())
        .binding_for_session(&session)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        binding.client_session_key,
        r#"["company-choice","native-id"]"#
    );
    app.shutdown().notify();
    tokio::time::timeout(std::time::Duration::from_secs(5), tasks.join())
        .await
        .unwrap()
        .unwrap();
    let mut clients = ClientRegistry::default();
    clients.register(pontia_client_acp::registration_with_registry(
        config.clone(),
        server.registry(),
    ));
    let app = AppState::builder(pool.clone(), state_root.clone())
        .clients(clients.clone())
        .build();
    let tasks = clients
        .start_services(app.clone(), ClientServicePhase::Transport)
        .await
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if app
                .queries()
                .get_session(&session)
                .await
                .unwrap()
                .unwrap()
                .capabilities
                .accept_task
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("Registry recovery must work when the state root and workspace differ");
    assert_eq!(
        pontia_application::AgentBindingService::new(pool.clone())
            .binding_for_session(&session)
            .await
            .unwrap()
            .unwrap()
            .id,
        binding.id
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM session_runtimes WHERE session_id=?")
            .bind(&session)
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
    app.shutdown().notify();
    tokio::time::timeout(std::time::Duration::from_secs(5), tasks.join())
        .await
        .unwrap()
        .unwrap();
    let cold = server.registry();
    drop(server);
    let mut clients = ClientRegistry::default();
    clients.register(pontia_client_acp::registration_with_registry(config, cold));
    let app = AppState::builder(pool, state_root).clients(clients).build();
    let detail = app.queries().get_session(&session).await.unwrap().unwrap();
    assert!(!detail.capabilities.accept_task);
    assert!(!detail.capabilities.timeline);
    assert_eq!(detail.client_details["acp"]["agent_id"], "company-choice");
    assert_eq!(
        detail.client_details["acp"]["agent_info"]["name"],
        "fixture"
    );
}

async fn stalled_request_can_be_retried(download: bool) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let root = tempfile::tempdir().unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let document = json!({"agents":[{"id":"stalled","name":"Stalled","distribution":{"binary":{platform():{"archive":format!("{url}/peer.tar"),"cmd":"peer"}},"npx":{"package":"must-not-fallback"}}}]}).to_string();
    let (stalled, waiting) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let mut stalled = Some(stalled);
        let mut held = Vec::new();
        loop {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                let byte = stream.read_u8().await.unwrap();
                request.push(byte);
            }
            let is_archive = request.starts_with(b"GET /peer.tar ");
            if is_archive == download
                && let Some(signal) = stalled.take()
            {
                // Send headers but never the body: the total timeout must
                // cover stalled response reads, not just connection setup.
                stream
                    .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 999999\r\n\r\n")
                    .await
                    .unwrap();
                held.push(stream);
                signal.send(()).unwrap();
                continue;
            }
            let body = if is_archive {
                archive()
            } else {
                document.as_bytes().to_vec()
            };
            stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).as_bytes()).await.unwrap();
            stream.write_all(&body).await.unwrap();
        }
    });
    let registry = Registry::new(format!("{url}/registry"));
    let operation = tokio::spawn({
        let registry = registry.clone();
        let cache = root.path().join("cache");
        let cwd = root.path().to_path_buf();
        async move { registry.resolve(&config("stalled"), &cache, &cwd).await }
    });
    tokio::time::timeout(std::time::Duration::from_secs(5), waiting)
        .await
        .unwrap()
        .unwrap();
    assert!(!operation.is_finished());
    tokio::time::pause();
    tokio::time::advance(std::time::Duration::from_secs(if download {
        299
    } else {
        29
    }))
    .await;
    tokio::task::yield_now().await;
    assert!(
        !operation.is_finished(),
        "request must retain its allotted deadline"
    );
    tokio::time::advance(std::time::Duration::from_secs(2)).await;
    let result = tokio::time::timeout(std::time::Duration::from_secs(1), operation)
        .await
        .expect("stalled request must time out")
        .unwrap();
    assert!(matches!(result, Err(pontia_core::Error::Domain(_))));
    tokio::time::resume();
    let launch = registry
        .resolve(&config("stalled"), &root.path().join("cache"), root.path())
        .await
        .unwrap();
    let output = tokio::process::Command::new(launch.command)
        .env("REGISTRY_ENV", "retry")
        .arg("literal")
        .output()
        .await
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"retry\nliteral\n");
    server.abort();
    let _ = server.await;
}

#[tokio::test]
async fn stalled_registry_body_times_out_and_discovery_can_be_retried() {
    stalled_request_can_be_retried(false).await;
}

#[tokio::test]
async fn stalled_binary_body_times_out_without_fallback_and_install_can_be_retried() {
    stalled_request_can_be_retried(true).await;
}

#[tokio::test]
#[ignore = "live: requires network access to the official ACP Registry"]
async fn live_official_registry_exposes_agent_names_without_a_release_allowlist() {
    assert!(!Registry::default().name("gemini").await.unwrap().is_empty());
}
