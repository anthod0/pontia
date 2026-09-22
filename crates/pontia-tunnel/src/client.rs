use std::{fs::File, io::BufReader, path::Path, sync::Arc, time::Duration};

use futures_util::{SinkExt, StreamExt};
use rustls::{ClientConfig, RootCertStore};
use tokio::{
    net::TcpStream,
    sync::watch,
    time::{Instant, timeout},
};
use tokio_tungstenite::{
    Connector, MaybeTlsStream, WebSocketStream, connect_async_tls_with_config,
    tungstenite::{Message as WsMessage, protocol::WebSocketConfig},
};
use tracing::{info, warn};
use url::Url;

use crate::{
    DeviceIdentity, Error, Result,
    protocol::{self, Message},
};

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const IDLE_TIMEOUT: Duration = Duration::from_secs(45);

pub struct RemoteClient {
    url: Url,
    identity: DeviceIdentity,
    access_key: String,
    connector: Connector,
}

impl RemoteClient {
    pub fn new(
        url: &str,
        identity: DeviceIdentity,
        access_key: String,
        ca_certificate: Option<&Path>,
    ) -> Result<Self> {
        if access_key.trim().is_empty() || access_key.len() > protocol::MAX_ACCESS_KEY_BYTES {
            return Err(Error::Protocol(
                "remote access key must be non-empty and at most 512 bytes",
            ));
        }
        let url = Url::parse(url).map_err(|_| Error::Protocol("invalid edge URL"))?;
        if url.scheme() != "wss"
            || url.host_str().is_none()
            || url.path() != "/tunnel"
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(Error::Protocol("edge URL must be wss://host[:port]/tunnel"));
        }
        let mut roots = RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        if let Some(path) = ca_certificate {
            let certs = rustls_pemfile::certs(&mut BufReader::new(File::open(path)?))
                .collect::<std::io::Result<Vec<_>>>()?;
            if certs.is_empty() {
                return Err(Error::Protocol("CA certificate file is empty"));
            }
            for cert in certs {
                roots
                    .add(cert)
                    .map_err(|_| Error::Protocol("invalid CA certificate"))?;
            }
        }
        let tls =
            ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
                .with_safe_default_protocol_versions()
                .map_err(|_| Error::Protocol("invalid TLS versions"))?
                .with_root_certificates(roots)
                .with_no_client_auth();
        Ok(Self {
            url,
            identity,
            access_key,
            connector: Connector::Rustls(Arc::new(tls)),
        })
    }

    pub async fn run(self, mut shutdown: watch::Receiver<bool>) {
        tokio::select! {
            _ = shutdown.wait_for(|stop| *stop) => {}
            _ = self.reconnect() => {}
        }
    }

    async fn reconnect(&self) {
        let mut backoff = Duration::from_secs(1);
        loop {
            let started = Instant::now();
            if let Err(error) = self.connection().await {
                warn!(device_id = %self.identity.device_id(), %error, "edge connection ended");
            }
            if started.elapsed() >= Duration::from_secs(60) {
                backoff = Duration::from_secs(1);
            }
            // A bounded jitter keeps devices from retrying together after an edge restart.
            let jitter = protocol::nonce()
                .map(|n| u16::from_be_bytes([n[0], n[1]]) as u64 % 1000)
                .unwrap_or(0);
            tokio::time::sleep(backoff + Duration::from_millis(jitter)).await;
            backoff = (backoff * 2).min(Duration::from_secs(30));
        }
    }

    async fn connection(&self) -> Result<()> {
        let (mut socket, _) = timeout(
            CONNECT_TIMEOUT,
            connect_async_tls_with_config(
                self.url.as_str(),
                Some(
                    WebSocketConfig::default()
                        .max_message_size(Some(protocol::MAX_MESSAGE_BYTES))
                        .max_frame_size(Some(protocol::MAX_MESSAGE_BYTES)),
                ),
                false,
                Some(self.connector.clone()),
            ),
        )
        .await
        .map_err(|_| Error::Protocol("edge connection timed out"))??;
        timeout(CONNECT_TIMEOUT, async {
            let Message::Challenge {
                version: protocol::VERSION,
                nonce,
            } = receive(&mut socket).await?
            else {
                return Err(Error::Protocol(
                    "expected supported authentication challenge",
                ));
            };
            send(
                &mut socket,
                self.identity.authenticate(&nonce, &self.access_key),
            )
            .await?;
            let Message::Authenticated { device_id } = receive(&mut socket).await? else {
                return Err(Error::Protocol("expected authentication confirmation"));
            };
            if device_id != self.identity.device_id() {
                return Err(Error::Protocol("unexpected authenticated device"));
            }
            Ok(())
        })
        .await
        .map_err(|_| Error::Protocol("edge authentication timed out"))??;
        info!(device_id = %self.identity.device_id(), "edge device authenticated");
        loop {
            timeout(IDLE_TIMEOUT, async {
                match receive(&mut socket).await? {
                    Message::Ping { nonce } => send(&mut socket, Message::Pong { nonce }).await,
                    _ => Err(Error::Protocol("unexpected tunnel message")),
                }
            })
            .await
            .map_err(|_| Error::Protocol("edge heartbeat timed out"))??;
        }
    }
}

async fn receive(socket: &mut Socket) -> Result<Message> {
    match socket.next().await {
        Some(Ok(WsMessage::Text(text))) => Ok(serde_json::from_str(&text)?),
        Some(Err(error)) => Err(error.into()),
        _ => Err(Error::Protocol("expected tunnel control message")),
    }
}

async fn send(socket: &mut Socket, message: Message) -> Result<()> {
    socket
        .send(WsMessage::Text(serde_json::to_string(&message)?.into()))
        .await?;
    Ok(())
}
