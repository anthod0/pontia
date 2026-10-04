use std::time::Duration;

use reqwest::{Client, StatusCode, Url};
use serde::Deserialize;
use uuid::Uuid;

use crate::credential::valid_credential;

const REDEEM_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone)]
pub struct TicketRedeemer {
    tunnel_endpoint: Url,
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
    pub fn new(cloud_origin: &str, service_credential: String) -> anyhow::Result<Self> {
        let client = Client::builder().timeout(REDEEM_TIMEOUT).build()?;
        Self::with_client(cloud_origin, service_credential, client)
    }

    pub fn with_client(
        cloud_origin: &str,
        service_credential: String,
        client: Client,
    ) -> anyhow::Result<Self> {
        let mut origin = Url::parse(cloud_origin)?;
        anyhow::ensure!(
            origin.scheme() == "https"
                && origin.host_str().is_some()
                && origin.username().is_empty()
                && origin.password().is_none()
                && origin.query().is_none()
                && origin.fragment().is_none(),
            "cloud origin must be an HTTPS origin without credentials, query, or fragment"
        );
        anyhow::ensure!(
            valid_credential(&service_credential),
            "invalid edge service credential"
        );
        origin.set_path("/");
        let tunnel_endpoint = origin.join("api/edge/tunnel-tickets/redeem")?;
        Ok(Self {
            tunnel_endpoint,
            service_credential,
            client,
        })
    }

    pub(crate) async fn redeem(&self, ticket: &str) -> Result<Uuid, RedeemError> {
        let response = self.request(self.tunnel_endpoint.clone(), ticket).await?;
        match response.status() {
            StatusCode::OK => decode_response(response)
                .await
                .map(|response: RedeemResponse| response.device_id),
            StatusCode::UNAUTHORIZED => Err(RedeemError::Rejected),
            _ => Err(RedeemError::Unavailable),
        }
    }

    async fn request(&self, endpoint: Url, ticket: &str) -> Result<reqwest::Response, RedeemError> {
        self.client
            .post(endpoint)
            .bearer_auth(&self.service_credential)
            .json(&serde_json::json!({ "ticket": ticket }))
            .send()
            .await
            .map_err(|_| RedeemError::Unavailable)
    }
}

async fn decode_response<T: for<'de> Deserialize<'de>>(
    response: reqwest::Response,
) -> Result<T, RedeemError> {
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .map(str::trim);
    if content_type != Some("application/json") {
        return Err(RedeemError::Unavailable);
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|_| RedeemError::Unavailable)?;
    if bytes.len() > 4096 {
        return Err(RedeemError::Unavailable);
    }
    serde_json::from_slice(&bytes).map_err(|_| RedeemError::Unavailable)
}
