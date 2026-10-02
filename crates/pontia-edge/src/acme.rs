use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use instant_acme::{
    Account, AccountCredentials, AuthorizationStatus, ChallengeType, Identifier, LetsEncrypt,
    NewAccount, NewOrder, Order, OrderStatus, RetryPolicy,
};

use crate::{
    challenge::{ChallengeResponses, ChallengeServer},
    enrollment::{DnsOperation, HttpCloudClient},
    files::atomic_write,
};

pub struct CloudDnsChallenges<'a> {
    pub client: &'a HttpCloudClient,
    pub credential: &'a str,
    pub ticket: Option<&'a str>,
}

impl CloudDnsChallenges<'_> {
    async fn change(&self, operation: DnsOperation, value: &str) -> Result<()> {
        self.client
            .dns_challenge(self.credential, self.ticket, operation, value)
            .await
    }
}

pub async fn complete_dns_and_save(
    order: PreparedDnsOrder,
    hostname: &str,
    tls_path: &Path,
    dns: &CloudDnsChallenges<'_>,
) -> Result<()> {
    let value = order.value.clone();
    let result = async {
        let certificate = order.complete(hostname).await?;
        save_certificate(&certificate, tls_path)
    }
    .await;
    if let Some(value) = value
        && let Err(error) = dns.change(DnsOperation::Cleanup, &value).await
    {
        tracing::warn!(%error, "DNS challenge cleanup failed; scheduled cleanup will retry");
    }
    result
}

pub async fn issue_dns_and_save(
    issuer: &InstantAcmeIssuer,
    hostname: &str,
    tls_path: &Path,
    dns: &CloudDnsChallenges<'_>,
) -> Result<()> {
    let order = issuer.prepare_dns(hostname).await?;
    if let Some(value) = &order.value {
        dns.change(DnsOperation::Publish, value).await?;
    }
    complete_dns_and_save(order, hostname, tls_path, dns).await
}

pub async fn issue_http_and_save(
    issuer: &InstantAcmeIssuer,
    hostname: &str,
    tls_path: &Path,
    shared: Option<ChallengeResponses>,
) -> Result<()> {
    let listener = if shared.is_none() {
        Some(ChallengeServer::start_acme("0.0.0.0:80".parse().unwrap()).await?)
    } else {
        None
    };
    let responses = shared.unwrap_or_else(|| listener.as_ref().unwrap().responses());
    let result = issue_and_save(issuer, hostname, responses, tls_path).await;
    if let Some(listener) = listener {
        listener.stop().await?;
    }
    result
}

#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    Eq,
    PartialEq,
    serde::Deserialize,
    serde::Serialize,
    clap::ValueEnum,
)]
pub enum AcmeChallenge {
    #[default]
    #[serde(rename = "http-01")]
    #[value(name = "http-01")]
    Http01,
    #[serde(rename = "dns-01")]
    #[value(name = "dns-01")]
    Dns01,
}

pub struct PreparedDnsOrder {
    order: Order,
    pub value: Option<String>,
}

impl PreparedDnsOrder {
    pub async fn complete(self, hostname: &str) -> Result<IssuedCertificate> {
        self.complete_with_resolver(hostname, "https://cloudflare-dns.com/dns-query")
            .await
    }

    async fn complete_with_resolver(
        mut self,
        hostname: &str,
        resolver: &str,
    ) -> Result<IssuedCertificate> {
        if let Some(value) = &self.value {
            wait_for_txt(hostname, value, resolver).await?;
        }
        let mut authorizations = self.order.authorizations();
        while let Some(authorization) = authorizations.next().await {
            let mut authorization = authorization?;
            if authorization.status == AuthorizationStatus::Valid {
                continue;
            }
            authorization
                .challenge(ChallengeType::Dns01)
                .context("ACME server did not offer DNS-01")?
                .set_ready()
                .await?;
        }
        finish_order(&mut self.order).await
    }
}

async fn wait_for_txt(hostname: &str, value: &str, resolver: &str) -> Result<()> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;
    for _ in 0..120 {
        let response = client
            .get(resolver)
            .query(&[
                ("name", format!("_acme-challenge.{hostname}")),
                ("type", "TXT".to_owned()),
            ])
            .header("accept", "application/dns-json")
            .send()
            .await;
        if let Ok(response) = response
            && let Ok(body) = response.json::<serde_json::Value>().await
            && body["Answer"].as_array().is_some_and(|answers| {
                answers.iter().any(|answer| {
                    answer["type"] == 16
                        && answer["data"]
                            .as_str()
                            .is_some_and(|data| data.trim_matches('"') == value)
                })
            })
        {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }
    anyhow::bail!("DNS-01 TXT record did not propagate")
}

async fn finish_order(order: &mut Order) -> Result<IssuedCertificate> {
    let status = order
        .poll_ready(&RetryPolicy::default())
        .await
        .context("ACME validation failed")?;
    anyhow::ensure!(
        status == OrderStatus::Ready,
        "ACME order did not become ready"
    );
    let private_key_pem = order
        .finalize()
        .await
        .context("failed to finalize ACME order")?;
    let certificate_pem = order
        .poll_certificate(&RetryPolicy::default())
        .await
        .context("failed to download issued certificate")?;
    Ok(IssuedCertificate {
        certificate_pem,
        private_key_pem,
    })
}

pub const ACCOUNT_PATH: &str = "/etc/pontia/edge/acme-account.json";
pub const TLS_PATH: &str = "/etc/pontia/edge/tls.pem";

#[derive(Debug)]
pub struct IssuedCertificate {
    pub certificate_pem: String,
    pub private_key_pem: String,
}

#[allow(async_fn_in_trait)]
pub trait CertificateIssuer {
    async fn issue(
        &self,
        hostname: &str,
        challenges: ChallengeResponses,
    ) -> Result<IssuedCertificate>;
}

pub struct InstantAcmeIssuer {
    account_path: PathBuf,
    directory_url: String,
}

impl InstantAcmeIssuer {
    pub fn production(account_path: impl Into<PathBuf>) -> Self {
        Self {
            account_path: account_path.into(),
            directory_url: LetsEncrypt::Production.url().to_owned(),
        }
    }

    pub fn staging(account_path: impl Into<PathBuf>) -> Self {
        Self {
            account_path: account_path.into(),
            directory_url: LetsEncrypt::Staging.url().to_owned(),
        }
    }

    pub async fn prepare_dns(&self, hostname: &str) -> Result<PreparedDnsOrder> {
        prepare_dns_order(self.account().await?, hostname).await
    }

    async fn account(&self) -> Result<Account> {
        self.load_account(Account::builder()?).await
    }
}

async fn prepare_dns_order(account: Account, hostname: &str) -> Result<PreparedDnsOrder> {
    let mut order = account
        .new_order(&NewOrder::new(&[Identifier::Dns(hostname.to_owned())]))
        .await?;
    let mut value = None;
    let mut authorizations = order.authorizations();
    while let Some(authorization) = authorizations.next().await {
        let mut authorization = authorization?;
        match authorization.status {
            AuthorizationStatus::Valid => continue,
            AuthorizationStatus::Pending => {}
            status => anyhow::bail!("ACME authorization has unexpected status: {status:?}"),
        }
        value = Some(
            authorization
                .challenge(ChallengeType::Dns01)
                .context("ACME server did not offer DNS-01")?
                .key_authorization()
                .dns_value(),
        );
    }
    Ok(PreparedDnsOrder { order, value })
}

impl InstantAcmeIssuer {
    async fn load_account(&self, builder: instant_acme::AccountBuilder) -> Result<Account> {
        if self.account_path.exists() {
            let serialized = fs::read(&self.account_path)
                .with_context(|| format!("failed to read {}", self.account_path.display()))?;
            let credentials: AccountCredentials = serde_json::from_slice(&serialized)
                .context("stored ACME account credentials are invalid")?;
            return builder
                .from_credentials(credentials)
                .await
                .context("failed to restore ACME account");
        }
        let (account, credentials) = builder
            .create(
                &NewAccount {
                    contact: &[],
                    terms_of_service_agreed: true,
                    only_return_existing: false,
                },
                self.directory_url.clone(),
                None,
            )
            .await
            .context("failed to create ACME account")?;
        atomic_write(
            &self.account_path,
            &serde_json::to_vec(&credentials)?,
            0o600,
        )?;
        Ok(account)
    }
}

impl CertificateIssuer for InstantAcmeIssuer {
    async fn issue(
        &self,
        hostname: &str,
        challenges: ChallengeResponses,
    ) -> Result<IssuedCertificate> {
        issue_http(self.account().await?, hostname, challenges).await
    }
}

async fn issue_http(
    account: Account,
    hostname: &str,
    challenges: ChallengeResponses,
) -> Result<IssuedCertificate> {
    let identifier = Identifier::Dns(hostname.to_owned());
    let mut order = account
        .new_order(&NewOrder::new(&[identifier]))
        .await
        .context("failed to create ACME order")?;
    let mut tokens = Vec::new();
    let result = async {
        let mut authorizations = order.authorizations();
        while let Some(authorization) = authorizations.next().await {
            let mut authorization = authorization.context("failed to load ACME authorization")?;
            match authorization.status {
                AuthorizationStatus::Valid => continue,
                AuthorizationStatus::Pending => {}
                status => anyhow::bail!("ACME authorization has unexpected status: {status:?}"),
            }
            let mut challenge = authorization
                .challenge(ChallengeType::Http01)
                .context("ACME server did not offer HTTP-01")?;
            let token = challenge.token.clone();
            let response = challenge.key_authorization().as_str().to_owned();
            challenges.set(token.clone(), response).await;
            tokens.push(token);
            challenge
                .set_ready()
                .await
                .context("failed to start ACME HTTP-01 challenge")?;
        }
        finish_order(&mut order).await
    }
    .await;
    for token in &tokens {
        challenges.remove(token).await;
    }
    result
}

pub async fn issue_and_save<I: CertificateIssuer>(
    issuer: &I,
    hostname: &str,
    challenges: ChallengeResponses,
    tls_path: &Path,
) -> Result<()> {
    let certificate = issuer.issue(hostname, challenges).await?;
    save_certificate(&certificate, tls_path)
}

pub fn save_certificate(certificate: &IssuedCertificate, tls_path: &Path) -> Result<()> {
    let mut pem = certificate.certificate_pem.as_bytes().to_vec();
    if !pem.ends_with(b"\n") {
        pem.push(b'\n');
    }
    pem.extend_from_slice(certificate.private_key_pem.as_bytes());
    atomic_write(tls_path, &pem, 0o600)
}

#[cfg(test)]
#[path = "acme_protocol_tests.rs"]
mod protocol_tests;

#[cfg(test)]
mod tests {
    use std::{fs, os::unix::fs::PermissionsExt, sync::Mutex};

    use super::*;

    struct FakeIssuer(Mutex<Vec<String>>);

    impl CertificateIssuer for FakeIssuer {
        async fn issue(
            &self,
            hostname: &str,
            _challenges: ChallengeResponses,
        ) -> Result<IssuedCertificate> {
            self.0.lock().unwrap().push(hostname.to_owned());
            Ok(IssuedCertificate {
                certificate_pem: "certificate".to_owned(),
                private_key_pem: "private-key".to_owned(),
            })
        }
    }

    #[tokio::test]
    async fn fake_issuer_writes_private_managed_files_without_network() {
        let test_root = tempfile::tempdir().unwrap();
        let tls_path = test_root.path().join("tls.pem");
        let issuer = FakeIssuer(Mutex::new(Vec::new()));

        issue_and_save(
            &issuer,
            "brave-silver-atlas.edge.pontia.dev",
            ChallengeResponses::default(),
            &tls_path,
        )
        .await
        .unwrap();

        assert_eq!(
            issuer.0.lock().unwrap().as_slice(),
            ["brave-silver-atlas.edge.pontia.dev"]
        );
        assert_eq!(
            fs::read_to_string(&tls_path).unwrap(),
            "certificate\nprivate-key"
        );
        assert_eq!(
            fs::metadata(tls_path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}
