use std::{fs, path::Path};

use anyhow::{Context, Result};
use reqwest::Url;
use serde::{Deserialize, Serialize};

use crate::files::atomic_write;

pub const CONFIG_PATH: &str = "/etc/pontia/edge/config.json";
pub const DATABASE_PATH: &str = "/etc/pontia/edge/edge.sqlite3";

fn default_bootstrap_origin() -> String {
    "https://pontia.dev".to_owned()
}

fn default_dashboard_origin() -> String {
    "https://app.pontia.dev".to_owned()
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ServiceConfig {
    pub cloud_origin: String,
    pub hostname: String,
    #[serde(default = "default_bootstrap_origin")]
    pub browser_bootstrap_origin: String,
    #[serde(default = "default_dashboard_origin")]
    pub browser_dashboard_origin: String,
}

impl ServiceConfig {
    pub fn save(&self, path: &Path) -> Result<()> {
        let mut serialized = serde_json::to_vec(self)?;
        serialized.push(b'\n');
        atomic_write(path, &serialized, 0o600)
    }

    pub fn read(path: &Path) -> Result<Self> {
        let config: Self = serde_json::from_slice(
            &fs::read(path).with_context(|| format!("failed to read {}", path.display()))?,
        )
        .context("edge service config is invalid")?;
        hostname_from_tunnel_url(&format!("wss://{}/tunnel", config.hostname))?;
        let origin =
            Url::parse(&config.cloud_origin).context("configured Cloud origin is invalid")?;
        anyhow::ensure!(
            origin.scheme() == "https" && origin.path() == "/",
            "configured Cloud origin is invalid"
        );
        validate_browser_origin(&config.browser_bootstrap_origin)?;
        validate_browser_origin(&config.browser_dashboard_origin)?;
        Ok(config)
    }
}

fn validate_browser_origin(value: &str) -> Result<()> {
    let origin = Url::parse(value).context("configured browser origin is invalid")?;
    anyhow::ensure!(
        matches!(origin.scheme(), "http" | "https")
            && origin.host_str().is_some()
            && origin.username().is_empty()
            && origin.password().is_none()
            && origin.path() == "/"
            && origin.query().is_none()
            && origin.fragment().is_none()
            && origin.as_str().strip_suffix('/') == Some(value),
        "configured browser origin is invalid"
    );
    Ok(())
}

pub fn hostname_from_tunnel_url(tunnel_url: &str) -> Result<String> {
    let url = Url::parse(tunnel_url).context("Cloud returned an invalid tunnel URL")?;
    anyhow::ensure!(
        url.scheme() == "wss"
            && url.username().is_empty()
            && url.password().is_none()
            && url.port().is_none()
            && url.path() == "/tunnel"
            && url.query().is_none()
            && url.fragment().is_none(),
        "Cloud returned a non-canonical tunnel URL"
    );
    let hostname = url.host_str().context("tunnel URL has no hostname")?;
    let label = hostname
        .strip_suffix(".edge.pontia.dev")
        .context("tunnel URL is outside the managed edge zone")?;
    anyhow::ensure!(
        !label.is_empty()
            && label.len() <= 63
            && !label.contains('.')
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'),
        "tunnel URL has an invalid edge hostname"
    );
    Ok(hostname.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_the_managed_canonical_tunnel_url() {
        assert_eq!(
            hostname_from_tunnel_url("wss://brave-silver-atlas.edge.pontia.dev/tunnel").unwrap(),
            "brave-silver-atlas.edge.pontia.dev"
        );
        for invalid in [
            "ws://brave-silver-atlas.edge.pontia.dev/tunnel",
            "wss://brave-silver-atlas.edge.pontia.dev:444/tunnel",
            "wss://two.parts.edge.pontia.dev/tunnel",
            "wss://brave-silver-atlas.edge.pontia.dev/other",
            "wss://brave-silver-atlas.edge.pontia.dev/tunnel?q=1",
            "wss://example.com/tunnel",
        ] {
            assert!(
                hostname_from_tunnel_url(invalid).is_err(),
                "accepted {invalid}"
            );
        }
    }
}
