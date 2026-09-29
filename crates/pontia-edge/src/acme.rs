use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use instant_acme::{
    Account, AccountCredentials, AuthorizationStatus, ChallengeType, Identifier, LetsEncrypt,
    NewAccount, NewOrder, OrderStatus, RetryPolicy,
};

use crate::{challenge::ChallengeResponses, files::atomic_write};

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
    directory: LetsEncrypt,
}

impl InstantAcmeIssuer {
    pub fn production(account_path: impl Into<PathBuf>) -> Self {
        Self {
            account_path: account_path.into(),
            directory: LetsEncrypt::Production,
        }
    }

    pub fn staging(account_path: impl Into<PathBuf>) -> Self {
        Self {
            account_path: account_path.into(),
            directory: LetsEncrypt::Staging,
        }
    }

    async fn account(&self) -> Result<Account> {
        if self.account_path.exists() {
            let serialized = fs::read(&self.account_path)
                .with_context(|| format!("failed to read {}", self.account_path.display()))?;
            let credentials: AccountCredentials = serde_json::from_slice(&serialized)
                .context("stored ACME account credentials are invalid")?;
            return Account::builder()?
                .from_credentials(credentials)
                .await
                .context("failed to restore ACME account");
        }
        let (account, credentials) = Account::builder()?
            .create(
                &NewAccount {
                    contact: &[],
                    terms_of_service_agreed: true,
                    only_return_existing: false,
                },
                self.directory.url().to_owned(),
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
        let account = self.account().await?;
        let identifier = Identifier::Dns(hostname.to_owned());
        let mut order = account
            .new_order(&NewOrder::new(&[identifier]))
            .await
            .context("failed to create ACME order")?;
        let mut tokens = Vec::new();
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
            challenge
                .set_ready()
                .await
                .context("failed to start ACME HTTP-01 challenge")?;
            tokens.push(token);
        }
        let status = order
            .poll_ready(&RetryPolicy::default())
            .await
            .context("ACME HTTP-01 validation failed")?;
        for token in &tokens {
            challenges.remove(token).await;
        }
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
