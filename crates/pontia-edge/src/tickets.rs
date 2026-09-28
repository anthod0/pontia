use std::time::Duration;

use reqwest::{Client, StatusCode, Url};
use serde::Deserialize;
use uuid::Uuid;

const REDEEM_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone)]
pub struct TicketRedeemer {
    endpoint: Url,
    service_credential: String,
    client: Client,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum RedeemError {
    Rejected,
    Unavailable,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RedeemResponse {
    device_id: Uuid,
}

impl TicketRedeemer {
    pub fn new(website_origin: &str, service_credential: String) -> anyhow::Result<Self> {
        let client = Client::builder().timeout(REDEEM_TIMEOUT).build()?;
        Self::with_client(website_origin, service_credential, client)
    }

    pub fn with_client(
        website_origin: &str,
        service_credential: String,
        client: Client,
    ) -> anyhow::Result<Self> {
        let mut origin = Url::parse(website_origin)?;
        anyhow::ensure!(
            origin.scheme() == "https"
                && origin.host_str().is_some()
                && origin.username().is_empty()
                && origin.password().is_none()
                && origin.query().is_none()
                && origin.fragment().is_none(),
            "website origin must be an HTTPS origin without credentials, query, or fragment"
        );
        anyhow::ensure!(
            valid_edge_credential(&service_credential),
            "invalid edge service credential"
        );
        origin.set_path("/");
        let endpoint = origin.join("api/edge/tunnel-tickets/redeem")?;
        Ok(Self {
            endpoint,
            service_credential,
            client,
        })
    }

    pub(crate) async fn redeem(&self, ticket: &str) -> Result<Uuid, RedeemError> {
        let response = self
            .client
            .post(self.endpoint.clone())
            .bearer_auth(&self.service_credential)
            .json(&serde_json::json!({ "ticket": ticket }))
            .send()
            .await
            .map_err(|_| RedeemError::Unavailable)?;
        match response.status() {
            StatusCode::OK => response
                .json::<RedeemResponse>()
                .await
                .map(|response| response.device_id)
                .map_err(|_| RedeemError::Unavailable),
            StatusCode::UNAUTHORIZED => Err(RedeemError::Rejected),
            _ => Err(RedeemError::Unavailable),
        }
    }
}

fn valid_edge_credential(value: &str) -> bool {
    let mut parts = value.split('_');
    parts.next() == Some("pec")
        && parts.next() == Some("v1")
        && parts.next().is_some_and(|id| !id.is_empty())
        && parts.next().is_some_and(|secret| {
            secret.len() == 43
                && secret
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        })
        && parts.next().is_none()
}
