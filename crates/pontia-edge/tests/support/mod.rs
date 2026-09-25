pub mod records;

use std::{path::PathBuf, sync::Arc, time::Duration};

use axum_server::{Handle, tls_rustls::RustlsConfig};
use futures_util::{SinkExt, StreamExt};
use pontia_edge::{ConnectionLimits, DeviceRegistry, Edge};
use pontia_tunnel::{DeviceIdentity, protocol::Message};
use rustls::{ClientConfig, RootCertStore, ServerConfig, pki_types::PrivatePkcs8KeyDer};
use tokio::{net::TcpStream, task::JoinHandle, time::timeout};
use tokio_tungstenite::{
    Connector, MaybeTlsStream, WebSocketStream, connect_async_tls_with_config,
    tungstenite::Message as WsMessage,
};

pub type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

pub struct TestEdge {
    pub root: tempfile::TempDir,
    pub edge: Edge,
    pub records: sqlx::SqlitePool,
    pub url: String,
    pub connector: Connector,
    pub ca_path: PathBuf,
    handle: Handle<std::net::SocketAddr>,
    task: JoinHandle<std::io::Result<()>>,
}

impl TestEdge {
    pub async fn start() -> Self {
        let root = tempfile::tempdir().unwrap();
        let devices = DeviceRegistry::open(&root.path().join("edge.db"))
            .await
            .unwrap();
        let records = sqlx::SqlitePool::connect_with(
            sqlx::sqlite::SqliteConnectOptions::new().filename(root.path().join("edge.db")),
        )
        .await
        .unwrap();
        let edge = Edge::new(
            devices,
            ConnectionLimits {
                max_pending: 2,
                auth_timeout: Duration::from_millis(500),
                heartbeat_interval: Duration::from_millis(100),
                pong_timeout: Duration::from_millis(200),
            },
        );
        let rcgen::CertifiedKey { cert, signing_key } =
            rcgen::generate_simple_self_signed(vec!["127.0.0.1".into()]).unwrap();
        let ca_path = root.path().join("ca.pem");
        std::fs::write(&ca_path, cert.pem()).unwrap();
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let server = ServerConfig::builder_with_provider(provider.clone())
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(
                vec![cert.der().clone()],
                PrivatePkcs8KeyDer::from(signing_key.serialize_der()).into(),
            )
            .unwrap();
        let mut roots = RootCertStore::empty();
        roots.add(cert.der().clone()).unwrap();
        let client = ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_root_certificates(roots)
            .with_no_client_auth();
        let connector = Connector::Rustls(Arc::new(client));
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!(
            "wss://127.0.0.1:{}/tunnel",
            listener.local_addr().unwrap().port()
        );
        let handle = Handle::new();
        let server =
            axum_server::from_tcp_rustls(listener, RustlsConfig::from_config(Arc::new(server)))
                .unwrap()
                .handle(handle.clone())
                .serve(edge.router().into_make_service());
        let task = tokio::spawn(server);
        Self {
            root,
            edge,
            records,
            url,
            connector,
            ca_path,
            handle,
            task,
        }
    }

    pub async fn connect(&self) -> Socket {
        timeout(
            Duration::from_secs(3),
            connect_async_tls_with_config(&self.url, None, false, Some(self.connector.clone())),
        )
        .await
        .unwrap()
        .unwrap()
        .0
    }

    pub async fn seed_device(&self, identity: &DeviceIdentity) {
        records::device(&self.records, identity).await;
    }

    pub async fn authenticate(&self, identity: &DeviceIdentity) -> Socket {
        let mut socket = self.connect().await;
        let Message::Challenge { nonce, .. } = receive(&mut socket).await else {
            panic!()
        };
        send(&mut socket, identity.authenticate(&nonce)).await;
        assert!(
            matches!(receive(&mut socket).await, Message::Authenticated { device_id } if device_id == identity.device_id())
        );
        socket
    }
}

impl Drop for TestEdge {
    fn drop(&mut self) {
        self.edge.shutdown();
        self.handle.shutdown();
        self.task.abort();
    }
}

pub async fn receive(socket: &mut Socket) -> Message {
    let message = timeout(Duration::from_secs(3), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let WsMessage::Text(text) = message else {
        panic!("expected text, got {message:?}")
    };
    serde_json::from_str(&text).unwrap()
}

pub async fn send(socket: &mut Socket, message: Message) {
    socket
        .send(WsMessage::Text(
            serde_json::to_string(&message).unwrap().into(),
        ))
        .await
        .unwrap();
}

pub async fn closed(socket: &mut Socket) {
    match timeout(Duration::from_secs(3), socket.next())
        .await
        .unwrap()
    {
        None | Some(Err(_)) | Some(Ok(WsMessage::Close(_))) => {}
        other => panic!("expected closed connection, got {other:?}"),
    }
}

pub async fn wait_for(mut condition: impl FnMut() -> bool) {
    timeout(Duration::from_secs(5), async {
        while !condition() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("condition should become true");
}
