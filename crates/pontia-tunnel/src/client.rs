use std::{
    collections::hash_map::DefaultHasher,
    fs,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};

use reqwest::{Client, StatusCode};
use rustls::{ClientConfig, RootCertStore};
use serde::Deserialize;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use tokio::{
    sync::watch,
    time::{Instant, timeout},
};
use tokio_tungstenite::{
    Connector, connect_async_tls_with_config,
    tungstenite::{client::IntoClientRequest, http::HeaderValue, protocol::WebSocketConfig},
};
use tracing::{info, warn};
use url::Url;
use uuid::Uuid;

use crate::{DeviceRequestHandler, Error, Result, protocol, transport::serve_device};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
#[cfg(not(test))]
const CREDENTIAL_WATCH_INTERVAL: Duration = Duration::from_secs(5);
#[cfg(test)]
const CREDENTIAL_WATCH_INTERVAL: Duration = Duration::from_millis(10);
const STABLE_CONNECTION: Duration = Duration::from_secs(60);

pub struct RemoteClient {
    cloud_origin: Url,
    device_id: Uuid,
    credential_path: PathBuf,
    http: Client,
    connector: Connector,
    handler: DeviceRequestHandler,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredCredential {
    token: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TicketResponse {
    ticket: String,
    tunnel_url: String,
    expires_at: String,
}

#[derive(Debug, Clone, Copy)]
enum AttemptError {
    AuthenticationPaused(CredentialFingerprint),
    Temporary(&'static str),
    ConnectionEnded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CredentialFingerprint(Option<u64>);

impl RemoteClient {
    pub fn new(
        cloud_origin: &str,
        device_id: Uuid,
        pontia_home: &Path,
        handler: DeviceRequestHandler,
    ) -> Result<Self> {
        let http = Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|_| Error::Protocol("failed to create cloud client"))?;
        let roots = RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let tls =
            ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
                .with_safe_default_protocol_versions()
                .map_err(|_| Error::Protocol("invalid TLS versions"))?
                .with_root_certificates(roots)
                .with_no_client_auth();
        Self::with_clients(
            cloud_origin,
            device_id,
            pontia_home,
            http,
            Connector::Rustls(Arc::new(tls)),
            handler,
        )
    }

    pub fn with_clients(
        cloud_origin: &str,
        device_id: Uuid,
        pontia_home: &Path,
        http: Client,
        connector: Connector,
        handler: DeviceRequestHandler,
    ) -> Result<Self> {
        let mut cloud_origin =
            Url::parse(cloud_origin).map_err(|_| Error::Protocol("invalid cloud origin"))?;
        if cloud_origin.scheme() != "https"
            || cloud_origin.host_str().is_none()
            || !cloud_origin.username().is_empty()
            || cloud_origin.password().is_some()
            || cloud_origin.query().is_some()
            || cloud_origin.fragment().is_some()
        {
            return Err(Error::Protocol(
                "cloud origin must be an HTTPS origin without credentials, query, or fragment",
            ));
        }
        cloud_origin.set_path("/");
        Ok(Self {
            cloud_origin,
            device_id,
            credential_path: pontia_home.join("auth.json"),
            http,
            connector,
            handler,
        })
    }

    pub async fn run(self, mut shutdown: watch::Receiver<bool>) {
        let mut backoff = Duration::from_secs(1);
        loop {
            let started = Instant::now();
            let outcome = self.attempt(shutdown.clone()).await;
            if *shutdown.borrow() {
                return;
            }
            let connected_for = started.elapsed();
            let delay = match outcome {
                AttemptError::AuthenticationPaused(fingerprint) => {
                    warn!(device_id = %self.device_id, "remote connection paused; login or device registration needs attention");
                    tokio::select! {
                        biased;
                        _ = shutdown.wait_for(|stop| *stop) => return,
                        _ = wait_for_credential_change(&self.credential_path, fingerprint) => {}
                    }
                    continue;
                }
                AttemptError::Temporary(reason) => {
                    warn!(device_id = %self.device_id, reason, "remote connection attempt failed");
                    let delay = jittered(backoff);
                    backoff = (backoff * 2).min(Duration::from_secs(30));
                    delay
                }
                AttemptError::ConnectionEnded => {
                    info!(device_id = %self.device_id, "remote connection ended");
                    if connected_for >= STABLE_CONNECTION {
                        backoff = Duration::from_secs(1);
                    }
                    let delay = jittered(backoff);
                    backoff = (backoff * 2).min(Duration::from_secs(30));
                    delay
                }
            };
            tokio::select! {
                biased;
                _ = shutdown.wait_for(|stop| *stop) => return,
                _ = tokio::time::sleep(delay) => {}
            }
        }
    }

    async fn attempt(&self, mut shutdown: watch::Receiver<bool>) -> AttemptError {
        let ticket = tokio::select! {
            biased;
            _ = shutdown.wait_for(|stop| *stop) => return AttemptError::ConnectionEnded,
            ticket = self.issue_ticket() => ticket,
        };
        let (ticket, tunnel_url) = match ticket {
            Ok(ticket) => ticket,
            Err(error) => return error,
        };
        match self.connection(&tunnel_url, &ticket, shutdown).await {
            Ok(()) => AttemptError::ConnectionEnded,
            Err(error) => error,
        }
    }

    async fn issue_ticket(&self) -> std::result::Result<(String, Url), AttemptError> {
        let (credential, fingerprint) = read_credential(&self.credential_path)?;
        let endpoint = self
            .cloud_origin
            .join(&format!(
                "api/remote/devices/{}/tunnel-tickets",
                self.device_id
            ))
            .map_err(|_| AttemptError::Temporary("invalid ticket endpoint"))?;
        let response = self
            .http
            .post(endpoint)
            .bearer_auth(credential)
            .send()
            .await
            .map_err(|_| AttemptError::Temporary("ticket service unavailable"))?;
        match response.status() {
            StatusCode::OK => {}
            StatusCode::UNAUTHORIZED | StatusCode::NOT_FOUND => {
                return Err(AttemptError::AuthenticationPaused(fingerprint));
            }
            _ => return Err(AttemptError::Temporary("ticket request rejected")),
        }
        let response: TicketResponse = response
            .json()
            .await
            .map_err(|_| AttemptError::Temporary("invalid ticket response"))?;
        let expires_at = OffsetDateTime::parse(&response.expires_at, &Rfc3339)
            .map_err(|_| AttemptError::Temporary("invalid ticket response"))?;
        if !valid_ticket(&response.ticket) || expires_at <= OffsetDateTime::now_utc() {
            return Err(AttemptError::Temporary("invalid ticket response"));
        }
        let tunnel_url = validate_tunnel_url(&response.tunnel_url)
            .map_err(|_| AttemptError::Temporary("invalid ticket response"))?;
        Ok((response.ticket, tunnel_url))
    }

    async fn connection(
        &self,
        tunnel_url: &Url,
        ticket: &str,
        shutdown: watch::Receiver<bool>,
    ) -> std::result::Result<(), AttemptError> {
        let mut request = tunnel_url
            .as_str()
            .into_client_request()
            .map_err(|_| AttemptError::Temporary("invalid tunnel request"))?;
        let value = HeaderValue::from_str(&format!("Bearer {ticket}"))
            .map_err(|_| AttemptError::Temporary("invalid tunnel ticket"))?;
        request.headers_mut().insert("authorization", value);
        request.headers_mut().insert(
            "sec-websocket-protocol",
            HeaderValue::from_static(protocol::SUBPROTOCOL),
        );
        let (socket, response) = timeout(
            CONNECT_TIMEOUT,
            connect_async_tls_with_config(
                request,
                Some(
                    WebSocketConfig::default()
                        .max_message_size(Some(protocol::MAX_WEBSOCKET_MESSAGE_BYTES))
                        .max_frame_size(Some(protocol::MAX_WEBSOCKET_MESSAGE_BYTES))
                        .write_buffer_size(protocol::ADAPTER_BUFFER_BYTES)
                        .max_write_buffer_size(protocol::MAX_WRITER_BUFFER_BYTES),
                ),
                false,
                Some(self.connector.clone()),
            ),
        )
        .await
        .map_err(|_| AttemptError::Temporary("edge connection timed out"))?
        .map_err(|_| AttemptError::Temporary("edge connection failed"))?;
        if response
            .headers()
            .get("sec-websocket-protocol")
            .and_then(|value| value.to_str().ok())
            != Some(protocol::SUBPROTOCOL)
        {
            return Err(AttemptError::Temporary(
                "edge selected an invalid tunnel protocol",
            ));
        }
        info!(device_id = %self.device_id, "remote device connected");
        serve_device(socket, self.handler.clone(), shutdown)
            .await
            .map_err(|_| AttemptError::ConnectionEnded)
    }
}

fn read_credential(
    path: &Path,
) -> std::result::Result<(String, CredentialFingerprint), AttemptError> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(_) => {
            return Err(AttemptError::AuthenticationPaused(CredentialFingerprint(
                None,
            )));
        }
    };
    let fingerprint = credential_fingerprint(&bytes);
    let stored: StoredCredential = serde_json::from_slice(&bytes)
        .map_err(|_| AttemptError::AuthenticationPaused(fingerprint))?;
    if !valid_credential(&stored.token) {
        return Err(AttemptError::AuthenticationPaused(fingerprint));
    }
    Ok((stored.token, fingerprint))
}

fn credential_fingerprint(bytes: &[u8]) -> CredentialFingerprint {
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    CredentialFingerprint(Some(hasher.finish()))
}

async fn wait_for_credential_change(path: &Path, previous: CredentialFingerprint) {
    loop {
        tokio::time::sleep(CREDENTIAL_WATCH_INTERVAL).await;
        let current = fs::read(path)
            .ok()
            .map(|bytes| credential_fingerprint(&bytes))
            .unwrap_or(CredentialFingerprint(None));
        if current != previous {
            return;
        }
    }
}

fn valid_credential(value: &str) -> bool {
    let mut parts = value.split('_');
    parts.next() == Some("ptr")
        && parts.next() == Some("v1")
        && parts.next().is_some_and(|id| !id.is_empty())
        && parts.next().is_some_and(|secret| secret.len() >= 43)
        && parts.next().is_none()
}

fn valid_ticket(value: &str) -> bool {
    let Some(secret) = value.strip_prefix("pet_v1_") else {
        return false;
    };
    if secret.len() != 43
        || !secret
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return false;
    }
    URL_SAFE_NO_PAD
        .decode(secret)
        .is_ok_and(|decoded| decoded.len() == 32 && URL_SAFE_NO_PAD.encode(decoded) == secret)
}

fn validate_tunnel_url(value: &str) -> Result<Url> {
    let url = Url::parse(value).map_err(|_| Error::Protocol("invalid tunnel URL"))?;
    if url.scheme() != "wss"
        || url.host_str().is_none()
        || url.path() != "/tunnel"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(Error::Protocol(
            "tunnel URL must be wss://host[:port]/tunnel",
        ));
    }
    Ok(url)
}

fn jittered(base: Duration) -> Duration {
    let mut bytes = [0; 2];
    let jitter = getrandom::fill(&mut bytes)
        .map(|()| u16::from_be_bytes(bytes) as u64 % 1000)
        .unwrap_or(0);
    base + Duration::from_millis(jitter)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{
        credential_fingerprint, valid_credential, valid_ticket, validate_tunnel_url,
        wait_for_credential_change,
    };

    #[test]
    fn credentials_and_tickets_use_distinct_strict_formats() {
        let secret = "v7-_".repeat(10) + "v78";
        let credential = format!("ptr_v1_session_{}", "A".repeat(43));
        let ticket = format!("pet_v1_{secret}");
        assert!(valid_credential(&credential));
        assert!(!valid_credential(&ticket));
        assert!(valid_ticket(&ticket));
        assert!(!valid_ticket(&credential));
        for invalid in [
            format!("ptt_v1_0195e7c1-1b22-7c33-9d44-123456789abc_{secret}"),
            format!("pet_v2_{secret}"),
            format!("{ticket}_extra"),
            format!("pet_v1_{}=", &secret[..42]),
            format!("pet_v1_{}", &secret[..42]),
        ] {
            assert!(!valid_ticket(&invalid), "{invalid}");
        }
    }

    #[test]
    fn tunnel_url_accepts_only_the_wss_tunnel_endpoint() {
        assert!(validate_tunnel_url("wss://edge.example/tunnel").is_ok());
        for value in [
            "ws://edge.example/tunnel",
            "wss://user:secret@edge.example/tunnel",
            "wss://edge.example/other",
            "wss://edge.example/tunnel?ticket=secret",
            "wss://edge.example/tunnel#fragment",
        ] {
            assert!(validate_tunnel_url(value).is_err(), "{value}");
        }
    }

    #[tokio::test]
    async fn authentication_pause_waits_for_the_credential_to_change() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("auth.json");
        std::fs::write(&path, b"old credential").unwrap();
        let wait = wait_for_credential_change(&path, credential_fingerprint(b"old credential"));
        tokio::pin!(wait);
        assert!(
            tokio::time::timeout(Duration::from_millis(25), &mut wait)
                .await
                .is_err()
        );
        std::fs::write(&path, b"new credential").unwrap();
        tokio::time::timeout(Duration::from_millis(50), wait)
            .await
            .unwrap();
    }
}
