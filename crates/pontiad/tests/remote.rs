#![cfg(unix)]

use std::{
    process::{Child, Command, Stdio},
    sync::Arc,
    time::Duration,
};

use axum_server::tls_rustls::RustlsConfig;
use pontia_edge::{ConnectionLimits, DeviceRegistry, Edge};
use pontia_tunnel::DeviceIdentity;
use rustls::{ServerConfig, pki_types::PrivatePkcs8KeyDer};
use sha2::{Digest, Sha256};

struct ChildGuard(Child);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[tokio::test]
async fn pontiad_loads_remote_config_authenticates_and_disconnects_on_sigterm() {
    let root = tempfile::tempdir().unwrap();
    let device_home = root.path().join("device");
    let identity =
        DeviceIdentity::load_or_create(&device_home.join("state/device-identity.json")).unwrap();
    let devices = DeviceRegistry::open(&root.path().join("edge.db"))
        .await
        .unwrap();
    let records = sqlx::SqlitePool::connect_with(
        sqlx::sqlite::SqliteConnectOptions::new().filename(root.path().join("edge.db")),
    )
    .await
    .unwrap();
    sqlx::query("INSERT INTO devices (device_id, public_key) VALUES (?, ?)")
        .bind(identity.device_id().to_string())
        .bind(identity.public_key().as_slice())
        .execute(&records)
        .await
        .unwrap();
    sqlx::query("INSERT INTO access_keys (key_id, secret_hash, device_id) VALUES (?, ?, ?)")
        .bind("fixture-key")
        .bind(Sha256::digest(b"fixture-remote-key").as_slice())
        .bind(identity.device_id().to_string())
        .execute(&records)
        .await
        .unwrap();
    let edge = Edge::new(devices, ConnectionLimits::default());
    let rcgen::CertifiedKey { cert, signing_key } =
        rcgen::generate_simple_self_signed(vec!["127.0.0.1".into()]).unwrap();
    let ca_path = root.path().join("ca.pem");
    std::fs::write(&ca_path, cert.pem()).unwrap();
    let tls =
        ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(
                vec![cert.der().clone()],
                PrivatePkcs8KeyDer::from(signing_key.serialize_der()).into(),
            )
            .unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = tokio::spawn(
        axum_server::from_tcp_rustls(listener, RustlsConfig::from_config(Arc::new(tls)))
            .unwrap()
            .serve(edge.router().into_make_service()),
    );
    std::fs::write(device_home.join("config.toml"), format!(
        "bind_addr = '127.0.0.1:0'\n[dashboard]\nsource = ''\n[remote]\nedge_url = 'wss://127.0.0.1:{port}/tunnel'\naccess_key = 'fixture-remote-key'\nca_certificate = '{}'\n", ca_path.display()
    )).unwrap();
    let log_path = root.path().join("pontiad.log");
    let log = std::fs::File::create(&log_path).unwrap();
    let mut child = ChildGuard(
        Command::new(env!("CARGO_BIN_EXE_pontiad"))
            .env("PONTIA_HOME", &device_home)
            .env("PONTIA_DASHBOARD_SOURCE", "")
            .env("PONTIA_RUN_MIGRATIONS", "true")
            .env("PONTIA_DEFAULT_CLIENT_TYPE", "pi")
            .stdout(Stdio::from(log.try_clone().unwrap()))
            .stderr(Stdio::from(log))
            .spawn()
            .unwrap(),
    );
    tokio::time::timeout(Duration::from_secs(10), async {
        while edge.online().connection_id(identity.device_id()).is_none() {
            assert!(
                child.0.try_wait().unwrap().is_none(),
                "{}",
                std::fs::read_to_string(&log_path).unwrap()
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("pontiad should authenticate over WSS");
    assert!(
        Command::new("kill")
            .arg("-TERM")
            .arg(child.0.id().to_string())
            .status()
            .unwrap()
            .success()
    );
    let status = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Some(status) = child.0.try_wait().unwrap() {
                break status;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("pontiad should stop on SIGTERM");
    assert!(
        status.success(),
        "{}",
        std::fs::read_to_string(&log_path).unwrap()
    );
    tokio::time::timeout(Duration::from_secs(2), async {
        while edge.online().connection_id(identity.device_id()).is_some() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("shutdown should remove device presence");
    edge.shutdown();
    server.abort();
    let _ = server.await;
}
