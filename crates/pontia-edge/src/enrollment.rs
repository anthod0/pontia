use std::path::Path;

use anyhow::{Context, Result};
use reqwest::{Client, StatusCode, Url};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::credential::{
    EdgeCredential, initialize_credential, read_edge_credential, replace_credential,
};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct EdgeIdentity {
    pub edge_id: Uuid,
    pub name: String,
    pub tunnel_url: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IdentityLookup {
    Registered(EdgeIdentity),
    Unauthorized,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InitializationResult {
    AlreadyRegistered(EdgeIdentity),
    Enrolled(EdgeIdentity),
}

#[allow(async_fn_in_trait)]
pub trait EnrollmentClient {
    async fn identity(&self, credential: &str) -> Result<IdentityLookup>;
    async fn enroll(&self, ticket: &str, credential: &str) -> Result<EdgeIdentity>;
}

#[allow(async_fn_in_trait)]
pub trait EdgeNetworkClient {
    async fn configure_network(&self, credential: &str, candidate_ipv4: &str) -> Result<String>;
    async fn verify_health(&self, credential: &str) -> Result<String>;
}

pub struct HttpCloudClient {
    client: Client,
    origin: Url,
}

impl HttpCloudClient {
    pub fn new(origin: &str) -> Result<Self> {
        let parsed = Url::parse(origin).context("--cloud-origin must be a valid URL")?;
        anyhow::ensure!(parsed.scheme() == "https", "--cloud-origin must use HTTPS");
        anyhow::ensure!(
            parsed.username().is_empty()
                && parsed.password().is_none()
                && parsed.query().is_none()
                && parsed.fragment().is_none()
                && parsed.path() == "/",
            "--cloud-origin must contain only an HTTPS origin"
        );
        Ok(Self {
            client: Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .context("failed to create Cloud client")?,
            origin: parsed,
        })
    }

    pub fn origin(&self) -> &Url {
        &self.origin
    }

    fn endpoint(&self, path: &str) -> Result<Url> {
        self.origin.join(path).context("failed to build Cloud URL")
    }
}

#[derive(Serialize)]
struct EnrollmentRequest<'a> {
    ticket: &'a str,
    service_credential: &'a str,
}

#[derive(Serialize)]
struct NetworkRequest<'a> {
    candidate_ipv4: &'a str,
}

#[derive(Deserialize)]
struct HostnameResponse {
    hostname: String,
}

impl EnrollmentClient for HttpCloudClient {
    async fn identity(&self, credential: &str) -> Result<IdentityLookup> {
        let response = self
            .client
            .get(self.endpoint("api/edge/me")?)
            .bearer_auth(credential)
            .send()
            .await
            .context("failed to query edge registration status")?;
        match response.status() {
            StatusCode::OK => Ok(IdentityLookup::Registered(
                response
                    .json()
                    .await
                    .context("Cloud returned an invalid edge identity")?,
            )),
            StatusCode::UNAUTHORIZED => Ok(IdentityLookup::Unauthorized),
            status => anyhow::bail!("Cloud returned an uncertain status: {status}"),
        }
    }

    async fn enroll(&self, ticket: &str, credential: &str) -> Result<EdgeIdentity> {
        let response = self
            .client
            .post(self.endpoint("api/edge/enroll")?)
            .json(&EnrollmentRequest {
                ticket,
                service_credential: credential,
            })
            .send()
            .await
            .context("failed to enroll edge")?;
        anyhow::ensure!(
            response.status() == StatusCode::OK,
            "Cloud rejected edge enrollment"
        );
        response
            .json()
            .await
            .context("Cloud returned an invalid enrollment response")
    }
}

impl EdgeNetworkClient for HttpCloudClient {
    async fn configure_network(&self, credential: &str, candidate_ipv4: &str) -> Result<String> {
        let response = self
            .client
            .post(self.endpoint("api/edge/network/configure")?)
            .bearer_auth(credential)
            .json(&NetworkRequest { candidate_ipv4 })
            .send()
            .await
            .context("failed to request edge network configuration")?;
        anyhow::ensure!(
            response.status() == StatusCode::OK,
            "Cloud rejected edge network configuration"
        );
        Ok(response
            .json::<HostnameResponse>()
            .await
            .context("Cloud returned an invalid network configuration response")?
            .hostname)
    }

    async fn verify_health(&self, credential: &str) -> Result<String> {
        let response = self
            .client
            .post(self.endpoint("api/edge/network/health")?)
            .bearer_auth(credential)
            .send()
            .await
            .context("failed to request public edge health verification")?;
        anyhow::ensure!(
            response.status() == StatusCode::OK,
            "Cloud could not verify public edge health"
        );
        Ok(response
            .json::<HostnameResponse>()
            .await
            .context("Cloud returned an invalid health response")?
            .hostname)
    }
}

pub async fn initialize_and_enroll<C: EnrollmentClient>(
    client: &C,
    credential_path: &Path,
    expected_edge_id: Uuid,
    ticket: &str,
) -> Result<InitializationResult> {
    let credential = if credential_path.exists() {
        let existing = read_edge_credential(credential_path)?;
        match client.identity(&existing.value).await? {
            IdentityLookup::Registered(identity) => {
                anyhow::ensure!(
                    identity.edge_id == existing.edge_id,
                    "Cloud returned a different edge identity"
                );
                return Ok(InitializationResult::AlreadyRegistered(identity));
            }
            IdentityLookup::Unauthorized if existing.edge_id == expected_edge_id => existing,
            IdentityLookup::Unauthorized => replace_credential(credential_path, expected_edge_id)?,
        }
    } else {
        initialize_credential(credential_path, expected_edge_id)?
    };

    enroll_credential(client, ticket, expected_edge_id, credential).await
}

async fn enroll_credential<C: EnrollmentClient>(
    client: &C,
    ticket: &str,
    expected_edge_id: Uuid,
    credential: EdgeCredential,
) -> Result<InitializationResult> {
    let identity = client.enroll(ticket, &credential.value).await?;
    anyhow::ensure!(
        identity.edge_id == expected_edge_id,
        "Cloud returned a different edge identity"
    );
    Ok(InitializationResult::Enrolled(identity))
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use axum::{
        Json, Router,
        extract::State,
        http::{HeaderMap, StatusCode, header},
        routing::{get, post},
    };
    use axum_server::{Handle, tls_rustls::RustlsConfig};
    use rustls::{ServerConfig, pki_types::PrivatePkcs8KeyDer};
    use serde_json::{Value, json};

    use super::*;
    use crate::credential::{initialize_credential, read_edge_credential};

    const FIRST_ID: &str = "0199791c-6600-7000-8000-000000000001";
    const SECOND_ID: &str = "0199791c-6600-7000-8000-000000000002";

    struct StubClient {
        lookup: Result<IdentityLookup, &'static str>,
        enrollment: Result<EdgeIdentity, &'static str>,
        enrolled_credentials: Mutex<Vec<String>>,
    }

    impl EnrollmentClient for StubClient {
        async fn identity(&self, _credential: &str) -> Result<IdentityLookup> {
            self.lookup.clone().map_err(anyhow::Error::msg)
        }

        async fn enroll(&self, _ticket: &str, credential: &str) -> Result<EdgeIdentity> {
            self.enrolled_credentials
                .lock()
                .unwrap()
                .push(credential.to_owned());
            self.enrollment.clone().map_err(anyhow::Error::msg)
        }
    }

    fn identity(id: &str) -> EdgeIdentity {
        EdgeIdentity {
            edge_id: Uuid::parse_str(id).unwrap(),
            name: "brave-silver-atlas".to_owned(),
            tunnel_url: "wss://brave-silver-atlas.edge.pontia.dev/tunnel".to_owned(),
        }
    }

    fn client(
        lookup: Result<IdentityLookup, &'static str>,
        enrollment: Result<EdgeIdentity, &'static str>,
    ) -> StubClient {
        StubClient {
            lookup,
            enrollment,
            enrolled_credentials: Mutex::new(Vec::new()),
        }
    }

    #[derive(Clone, Default)]
    struct RequestLog(Arc<Mutex<Vec<(String, String)>>>);

    async fn get_identity(headers: HeaderMap) -> Result<Json<Value>, StatusCode> {
        if headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            != Some("Bearer service-credential")
        {
            return Err(StatusCode::UNAUTHORIZED);
        }
        Ok(Json(
            json!({ "edge_id": FIRST_ID, "name": "brave-silver-atlas", "tunnel_url": "wss://brave-silver-atlas.edge.pontia.dev/tunnel" }),
        ))
    }

    async fn enroll(
        State(log): State<RequestLog>,
        Json(body): Json<Value>,
    ) -> Result<Json<Value>, StatusCode> {
        let ticket = body
            .get("ticket")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let credential = body
            .get("service_credential")
            .and_then(Value::as_str)
            .unwrap_or_default();
        log.0
            .lock()
            .unwrap()
            .push((ticket.to_owned(), credential.to_owned()));
        Ok(Json(
            json!({ "edge_id": FIRST_ID, "name": "brave-silver-atlas", "tunnel_url": "wss://brave-silver-atlas.edge.pontia.dev/tunnel" }),
        ))
    }

    #[tokio::test]
    async fn registered_credential_is_preserved_without_enrollment() {
        let test_root = tempfile::tempdir().unwrap();
        let path = test_root.path().join("credential");
        let first_id = Uuid::parse_str(FIRST_ID).unwrap();
        let existing = initialize_credential(&path, first_id).unwrap();
        let client = client(
            Ok(IdentityLookup::Registered(identity(FIRST_ID))),
            Err("must not enroll"),
        );

        let result = initialize_and_enroll(
            &client,
            &path,
            Uuid::parse_str(SECOND_ID).unwrap(),
            "ticket",
        )
        .await
        .unwrap();

        assert_eq!(
            result,
            InitializationResult::AlreadyRegistered(identity(FIRST_ID))
        );
        assert_eq!(read_edge_credential(&path).unwrap(), existing);
        assert!(client.enrolled_credentials.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn unauthorized_matching_credential_is_reused_for_enrollment() {
        let test_root = tempfile::tempdir().unwrap();
        let path = test_root.path().join("credential");
        let edge_id = Uuid::parse_str(FIRST_ID).unwrap();
        let existing = initialize_credential(&path, edge_id).unwrap();
        let client = client(Ok(IdentityLookup::Unauthorized), Ok(identity(FIRST_ID)));

        initialize_and_enroll(&client, &path, edge_id, "ticket")
            .await
            .unwrap();

        assert_eq!(read_edge_credential(&path).unwrap(), existing);
        assert_eq!(
            client.enrolled_credentials.lock().unwrap().as_slice(),
            [existing.value]
        );
    }

    #[tokio::test]
    async fn unauthorized_different_credential_is_atomically_replaced() {
        let test_root = tempfile::tempdir().unwrap();
        let path = test_root.path().join("credential");
        initialize_credential(&path, Uuid::parse_str(FIRST_ID).unwrap()).unwrap();
        let second_id = Uuid::parse_str(SECOND_ID).unwrap();
        let client = client(Ok(IdentityLookup::Unauthorized), Ok(identity(SECOND_ID)));

        initialize_and_enroll(&client, &path, second_id, "ticket")
            .await
            .unwrap();

        assert_eq!(read_edge_credential(&path).unwrap().edge_id, second_id);
    }

    #[tokio::test]
    async fn uncertain_lookup_preserves_the_existing_credential_and_stops() {
        let test_root = tempfile::tempdir().unwrap();
        let path = test_root.path().join("credential");
        let existing = initialize_credential(&path, Uuid::parse_str(FIRST_ID).unwrap()).unwrap();
        let client = client(Err("network unavailable"), Ok(identity(SECOND_ID)));

        assert!(
            initialize_and_enroll(
                &client,
                &path,
                Uuid::parse_str(SECOND_ID).unwrap(),
                "ticket"
            )
            .await
            .is_err()
        );
        assert_eq!(read_edge_credential(&path).unwrap(), existing);
        assert!(client.enrolled_credentials.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn enrollment_failure_keeps_the_new_credential_for_retry() {
        let test_root = tempfile::tempdir().unwrap();
        let path = test_root.path().join("credential");
        let edge_id = Uuid::parse_str(FIRST_ID).unwrap();
        let client = client(Ok(IdentityLookup::Unauthorized), Err("request failed"));

        assert!(
            initialize_and_enroll(&client, &path, edge_id, "ticket")
                .await
                .is_err()
        );
        assert_eq!(read_edge_credential(&path).unwrap().edge_id, edge_id);
    }

    #[tokio::test]
    async fn http_client_uses_the_identity_and_enrollment_contracts() {
        let rcgen::CertifiedKey { cert, signing_key } =
            rcgen::generate_simple_self_signed(vec!["127.0.0.1".into()]).unwrap();
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let tls = ServerConfig::builder_with_provider(provider)
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
        let origin = format!(
            "https://127.0.0.1:{}",
            listener.local_addr().unwrap().port()
        );
        let log = RequestLog::default();
        let router = Router::new()
            .route("/api/edge/me", get(get_identity))
            .route("/api/edge/enroll", post(enroll))
            .with_state(log.clone());
        let handle = Handle::new();
        let task = tokio::spawn(
            axum_server::from_tcp_rustls(listener, RustlsConfig::from_config(Arc::new(tls)))
                .unwrap()
                .handle(handle.clone())
                .serve(router.into_make_service()),
        );
        let client = HttpCloudClient {
            client: Client::builder()
                .add_root_certificate(reqwest::Certificate::from_der(cert.der()).unwrap())
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .unwrap(),
            origin: Url::parse(&origin).unwrap(),
        };

        assert_eq!(
            client.identity("service-credential").await.unwrap(),
            IdentityLookup::Registered(identity(FIRST_ID))
        );
        assert_eq!(
            client
                .enroll("deployment-ticket", "service-credential")
                .await
                .unwrap(),
            identity(FIRST_ID)
        );
        assert_eq!(
            log.0.lock().unwrap().as_slice(),
            [(
                "deployment-ticket".to_owned(),
                "service-credential".to_owned()
            )]
        );

        handle.shutdown();
        task.await.unwrap().unwrap();
    }

    #[test]
    fn cloud_origin_requires_a_bare_https_origin() {
        assert!(HttpCloudClient::new("https://pontia.example").is_ok());
        for invalid in [
            "http://pontia.example",
            "https://pontia.example/path",
            "https://user@pontia.example",
            "not a URL",
        ] {
            assert!(HttpCloudClient::new(invalid).is_err(), "accepted {invalid}");
        }
    }
}
