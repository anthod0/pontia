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
                Ok((Box::new(stream) as Self::Stream, router(challenges, false)))
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
