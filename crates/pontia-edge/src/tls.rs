use std::{future::Future, io, pin::Pin, time::Duration};

use axum::Router;
use axum_server::{
    accept::Accept,
    tls_rustls::{RustlsAcceptor, RustlsConfig},
};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::TcpStream,
};

use crate::challenge::{ChallengeResponses, router};

#[derive(Debug)]
pub struct EdgeCertResolver {
    suffix: String,
    control: RustlsConfig,
    preview: std::sync::RwLock<Option<RustlsConfig>>,
}

impl EdgeCertResolver {
    pub fn new(base: &str, control: RustlsConfig) -> std::sync::Arc<Self> {
        std::sync::Arc::new(Self {
            suffix: format!(".{base}"),
            control,
            preview: Default::default(),
        })
    }

    pub fn server(self: &std::sync::Arc<Self>) -> RustlsConfig {
        let mut config = (*self.control.get_inner()).clone();
        config.cert_resolver = self.clone();
        RustlsConfig::from_config(std::sync::Arc::new(config))
    }

    pub async fn load_preview(&self, pem: &std::path::Path) -> io::Result<()> {
        let config = RustlsConfig::from_pem_file(pem, pem).await?;
        *self.preview.write().unwrap() = Some(config);
        Ok(())
    }
}

impl rustls::server::ResolvesServerCert for EdgeCertResolver {
    fn resolve(
        &self,
        hello: rustls::server::ClientHello<'_>,
    ) -> Option<std::sync::Arc<rustls::sign::CertifiedKey>> {
        let Some(name) = hello.server_name().filter(|name| {
            name.strip_suffix(&self.suffix)
                .is_some_and(|label| !label.is_empty() && !label.contains('.'))
        }) else {
            return self.control.get_inner().cert_resolver.resolve(hello);
        };
        if let Some(preview) = self.preview.read().unwrap().as_ref() {
            return preview.get_inner().cert_resolver.resolve(hello);
        }
        // Operators may install one multi-SAN PEM for both host families.
        let name = rustls::pki_types::ServerName::try_from(name.to_owned()).ok()?;
        let key = self.control.get_inner().cert_resolver.resolve(hello)?;
        let cert = rustls::server::ParsedCertificate::try_from(key.end_entity_cert().ok()?).ok()?;
        rustls::client::verify_server_name(&cert, &name).ok()?;
        Some(key)
    }
}

pub trait ConnectionIo: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> ConnectionIo for T {}

/// Shares port 80 between TLS and HTTP-01 only while ACME responses are installed.
#[derive(Clone)]
pub struct AcmeAcceptor {
    tls: RustlsAcceptor,
    challenges: ChallengeResponses,
}

impl AcmeAcceptor {
    pub fn new(tls: RustlsConfig, challenges: ChallengeResponses) -> Self {
        Self {
            tls: RustlsAcceptor::new(tls),
            challenges,
        }
    }
}

impl Accept<TcpStream, Router> for AcmeAcceptor {
    type Stream = Box<dyn ConnectionIo>;
    type Service = Router;
    type Future = Pin<Box<dyn Future<Output = io::Result<(Self::Stream, Router)>> + Send>>;

    fn accept(&self, stream: TcpStream, service: Router) -> Self::Future {
        let tls = self.tls.clone();
        let challenges = self.challenges.clone();
        Box::pin(async move {
            let mut first = [0];
            let read = tokio::time::timeout(Duration::from_secs(5), stream.peek(&mut first))
                .await
                .map_err(|_| {
                    io::Error::new(
                        io::ErrorKind::TimedOut,
                        "TLS/ACME protocol detection timed out",
                    )
                })??;
            if read != 0 && first[0] == 22 {
                let (stream, service) = tls.accept(stream, service).await?;
                Ok((Box::new(stream) as Self::Stream, service))
            } else if read != 0 && challenges.is_active().await {
                Ok((Box::new(stream) as Self::Stream, router(challenges)))
            } else {
                Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "HTTP-01 is not active",
                ))
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::routing::get;

    #[tokio::test]
    async fn selects_and_reloads_only_the_single_label_preview_certificate() {
        let root = tempfile::tempdir().unwrap();
        let control_path = root.path().join("tls.pem");
        let preview_path = root.path().join("preview-tls.pem");
        let preview_name = "5173-device.demo.pontia.run";
        let nested_name = "nested.5173-device.demo.pontia.run";
        let write_cert = |path: &std::path::Path, names: Vec<String>| {
            let key = rcgen::generate_simple_self_signed(names).unwrap();
            std::fs::write(
                path,
                format!("{}{}", key.cert.pem(), key.signing_key.serialize_pem()),
            )
            .unwrap();
            reqwest::Certificate::from_der(key.cert.der()).unwrap()
        };
        let control = write_cert(
            &control_path,
            vec![
                "localhost".into(),
                "*.demo.pontia.run".into(),
                nested_name.into(),
            ],
        );
        let resolver = EdgeCertResolver::new(
            "demo.pontia.run",
            RustlsConfig::from_pem_file(&control_path, &control_path)
                .await
                .unwrap(),
        );
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(
            axum_server::from_tcp_rustls(listener, resolver.server())
                .unwrap()
                .serve(
                    Router::new()
                        .route("/", get(|| async { "ok" }))
                        .into_make_service(),
                ),
        );
        let request = |name: &'static str, cert| async move {
            reqwest::Client::builder()
                .no_proxy()
                .add_root_certificate(cert)
                .resolve(name, address)
                .build()
                .unwrap()
                .get(format!("https://{name}:{}/", address.port()))
                .send()
                .await
        };
        assert!(
            request(preview_name, control.clone())
                .await
                .unwrap()
                .status()
                .is_success()
        );
        for _ in 0..2 {
            let cert = write_cert(&preview_path, vec!["*.demo.pontia.run".into()]);
            resolver.load_preview(&preview_path).await.unwrap();
            assert!(
                request(preview_name, cert)
                    .await
                    .unwrap()
                    .status()
                    .is_success()
            );
            assert!(request(preview_name, control.clone()).await.is_err());
            assert!(
                request("localhost", control.clone())
                    .await
                    .unwrap()
                    .status()
                    .is_success()
            );
            assert!(
                request(nested_name, control.clone())
                    .await
                    .unwrap()
                    .status()
                    .is_success()
            );
        }
        server.abort();
    }

    #[tokio::test]
    async fn shared_listener_keeps_tls_available_and_exposes_only_active_acme_http() {
        let root = tempfile::tempdir().unwrap();
        let pem = root.path().join("tls.pem");
        let rcgen::CertifiedKey { cert, signing_key } =
            rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        std::fs::write(
            &pem,
            format!("{}{}", cert.pem(), signing_key.serialize_pem()),
        )
        .unwrap();
        let tls = RustlsConfig::from_pem_file(&pem, &pem).await.unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let responses = ChallengeResponses::default();
        let handle = axum_server::Handle::new();
        let task = tokio::spawn(
            axum_server::from_tcp(listener)
                .unwrap()
                .acceptor(AcmeAcceptor::new(tls, responses.clone()))
                .handle(handle.clone())
                .serve(
                    Router::new()
                        .route("/healthz", get(|| async { "ok" }))
                        .into_make_service(),
                ),
        );
        let client = reqwest::Client::builder()
            .add_root_certificate(reqwest::Certificate::from_der(cert.der()).unwrap())
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap();
        assert!(
            client
                .get(format!("http://{address}/healthz"))
                .send()
                .await
                .is_err()
        );
        for _ in 0..2 {
            responses.set("token".into(), "authorization".into()).await;
            assert_eq!(
                client
                    .get(format!("http://{address}/.well-known/acme-challenge/token"))
                    .send()
                    .await
                    .unwrap()
                    .text()
                    .await
                    .unwrap(),
                "authorization"
            );
            assert_eq!(
                client
                    .get(format!("http://{address}/healthz"))
                    .send()
                    .await
                    .unwrap()
                    .status(),
                404
            );
            assert_eq!(
                client
                    .get(format!("https://localhost:{}/healthz", address.port()))
                    .send()
                    .await
                    .unwrap()
                    .text()
                    .await
                    .unwrap(),
                "ok"
            );
            responses.remove("token").await;
            assert_eq!(
                client
                    .get(format!("http://{address}/.well-known/acme-challenge/token"))
                    .send()
                    .await
                    .unwrap()
                    .status(),
                404
            );
        }
        handle.shutdown();
        task.await.unwrap().unwrap();
    }
}
