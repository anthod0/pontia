use std::{fs, path::Path};

use anyhow::{Context, Result};
use pontia_tunnel::endpoint::{is_public_host, is_public_hostname};
use reqwest::Url;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

use crate::{acme::AcmeChallenge, files::atomic_write, port::validate_edge_port};

fn default_port() -> u16 {
    443
}

pub const CLOUD_ORIGIN: &str = "https://pontia.dev";
pub const CONFIG_PATH: &str = "/etc/pontia/edge/config.json";
fn default_dashboard_origin() -> String {
    "https://app.pontia.dev".to_owned()
}

const DEFAULT_PREVIEW_RANGE: &str = "20000-20099";
const DEFAULT_MAX_PREVIEW_LISTENERS: usize = 32;
const MAX_PREVIEW_LISTENERS: usize = 128;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum PortPreviewSetting {
    #[default]
    Auto,
    Disabled,
    Enabled(PortPreviewConfig),
}

impl PortPreviewSetting {
    pub fn parse(value: &str) -> std::result::Result<Self, String> {
        if value == "off" {
            return Ok(Self::Disabled);
        }
        let config = PortPreviewConfig {
            ranges: value.split(',').map(str::to_owned).collect(),
            max_listeners: DEFAULT_MAX_PREVIEW_LISTENERS,
        };
        config.allowed_ports(443)?;
        Ok(Self::Enabled(config))
    }

    pub fn enabled(&self) -> Option<&PortPreviewConfig> {
        match self {
            Self::Enabled(config) => Some(config),
            Self::Auto | Self::Disabled => None,
        }
    }

    fn is_auto(&self) -> bool {
        matches!(self, Self::Auto)
    }
}

impl Serialize for PortPreviewSetting {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Auto => serializer.serialize_unit(),
            Self::Disabled => serializer.serialize_bool(false),
            Self::Enabled(config) => config.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for PortPreviewSetting {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct Visitor;
        impl<'de> de::Visitor<'de> for Visitor {
            type Value = PortPreviewSetting;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("false or a preview port configuration")
            }

            fn visit_bool<E: de::Error>(self, value: bool) -> std::result::Result<Self::Value, E> {
                if value {
                    Err(E::custom("port_preview must be false or an object"))
                } else {
                    Ok(PortPreviewSetting::Disabled)
                }
            }

            fn visit_map<A: de::MapAccess<'de>>(
                self,
                map: A,
            ) -> std::result::Result<Self::Value, A::Error> {
                PortPreviewConfig::deserialize(de::value::MapAccessDeserializer::new(map))
                    .map(PortPreviewSetting::Enabled)
            }
        }
        deserializer.deserialize_any(Visitor)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PortPreviewConfig {
    pub ranges: Vec<String>,
    #[serde(default = "default_max_preview_listeners")]
    pub max_listeners: usize,
}

fn default_max_preview_listeners() -> usize {
    DEFAULT_MAX_PREVIEW_LISTENERS
}

impl PortPreviewConfig {
    pub fn default_pool() -> Self {
        Self {
            ranges: vec![DEFAULT_PREVIEW_RANGE.to_owned()],
            max_listeners: DEFAULT_MAX_PREVIEW_LISTENERS,
        }
    }

    pub fn allowed_ports(&self, control_port: u16) -> std::result::Result<Vec<u16>, String> {
        if self.ranges.is_empty() {
            return Err("preview port ranges cannot be empty".to_owned());
        }
        if !(1..=MAX_PREVIEW_LISTENERS).contains(&self.max_listeners) {
            return Err(format!(
                "preview max_listeners must be between 1 and {MAX_PREVIEW_LISTENERS}"
            ));
        }
        let mut ports = Vec::new();
        for range in &self.ranges {
            let (start, end) = match range.split_once('-') {
                Some((start, end)) => (parse_preview_port(start)?, parse_preview_port(end)?),
                None => {
                    let port = parse_preview_port(range)?;
                    (port, port)
                }
            };
            if start > end {
                return Err(format!("invalid preview port range {range}"));
            }
            ports.extend((start..=end).filter(|port| {
                *port != control_port && pontia_tunnel::endpoint::is_safe_port(*port)
            }));
        }
        ports.sort_unstable();
        ports.dedup();
        if ports.is_empty() {
            return Err("preview port ranges contain no usable ports".to_owned());
        }
        Ok(ports)
    }
}

fn parse_preview_port(value: &str) -> std::result::Result<u16, String> {
    let port = value
        .parse::<u16>()
        .map_err(|_| format!("invalid preview port {value}"))?;
    if port < 1024 || port.to_string() != value {
        return Err(format!(
            "preview port must be between 1024 and 65535: {value}"
        ));
    }
    Ok(port)
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ServiceConfig {
    pub hostname: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default)]
    pub acme_challenge: AcmeChallenge,
    #[serde(default = "default_dashboard_origin")]
    pub browser_dashboard_origin: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview_base_hostname: Option<String>,
    #[serde(default, skip_serializing_if = "PortPreviewSetting::is_auto")]
    pub port_preview: PortPreviewSetting,
}

impl ServiceConfig {
    pub fn save(&self, path: &Path) -> Result<()> {
        let mut serialized = serde_json::to_vec(self)?;
        serialized.push(b'\n');
        atomic_write(path, &serialized, 0o600)
    }

    pub fn read(path: &Path) -> Result<Self> {
        let mut config: Self = serde_json::from_slice(
            &fs::read(path).with_context(|| format!("failed to read {}", path.display()))?,
        )
        .context("edge service config is invalid")?;
        validate_edge_port(config.port).map_err(anyhow::Error::msg)?;
        anyhow::ensure!(
            is_public_host(&config.hostname),
            "configured edge host is not a public DNS name or unicast IP address"
        );
        if let Some(base) = &config.preview_base_hostname {
            anyhow::ensure!(
                is_public_hostname(base),
                "configured preview hostname is not a public DNS name"
            );
        }
        if matches!(config.port_preview, PortPreviewSetting::Auto) {
            config.port_preview = if config.hostname.parse::<std::net::IpAddr>().is_ok()
                && config.preview_base_hostname.is_none()
            {
                PortPreviewSetting::Enabled(PortPreviewConfig::default_pool())
            } else {
                PortPreviewSetting::Disabled
            };
        }
        anyhow::ensure!(
            config.preview_base_hostname.is_none() || config.port_preview.enabled().is_none(),
            "preview_base_hostname and port_preview cannot both be enabled"
        );
        if let Some(preview) = config.port_preview.enabled() {
            preview
                .allowed_ports(config.port)
                .map_err(anyhow::Error::msg)?;
        }
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
            && validate_edge_port(url.port().unwrap_or(443)).is_ok()
            && url.path() == "/tunnel"
            && url.query().is_none()
            && url.fragment().is_none(),
        "Cloud returned a non-canonical tunnel URL"
    );
    let serialized_host = url.host_str().context("tunnel URL has no host")?;
    let hostname = serialized_host
        .strip_prefix('[')
        .and_then(|host| host.strip_suffix(']'))
        .unwrap_or(serialized_host);
    anyhow::ensure!(
        is_public_host(hostname) && url.as_str() == tunnel_url,
        "tunnel URL has an invalid edge host"
    );
    Ok(hostname.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_configuration_survives_restart() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config.json");
        for (hostname, port, acme_challenge, preview) in [
            ("edge.example.com", 443, AcmeChallenge::Http01, None),
            ("8.8.8.8", 443, AcmeChallenge::Http01, None),
            ("2606:4700:4700::1111", 8443, AcmeChallenge::Http01, None),
            ("edge.example.com", 8443, AcmeChallenge::Http01, None),
            (
                "sg.edge.pontia.dev",
                8443,
                AcmeChallenge::Dns01,
                Some("sg.pontia.run"),
            ),
        ] {
            let config = ServiceConfig {
                hostname: hostname.to_owned(),
                port,
                acme_challenge,
                browser_dashboard_origin: default_dashboard_origin(),
                preview_base_hostname: preview.map(str::to_owned),
                port_preview: PortPreviewSetting::Disabled,
            };
            config.save(&path).unwrap();
            assert_eq!(ServiceConfig::read(&path).unwrap(), config);
        }
        std::fs::write(&path, br#"{"hostname":"edge.example.com"}"#).unwrap();
        let config = ServiceConfig::read(&path).unwrap();
        assert_eq!(config.preview_base_hostname, None);
        assert_eq!(config.port, 443);
        assert_eq!(config.acme_challenge, AcmeChallenge::Http01);
        config.save(&path).unwrap();
        assert!(
            !std::fs::read_to_string(&path)
                .unwrap()
                .contains("preview_base_hostname")
        );

        std::fs::write(
            &path,
            br#"{"cloud_origin":"https://old.example","hostname":"edge.example.com"}"#,
        )
        .unwrap();
        let config = ServiceConfig::read(&path).unwrap();
        config.save(&path).unwrap();
        assert!(
            !std::fs::read_to_string(&path)
                .unwrap()
                .contains("cloud_origin")
        );
    }

    #[test]
    fn existing_managed_edges_keep_dns_renewal_and_their_explicit_preview_hostname() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config.json");
        let stored = br#"{"hostname":"cn.edge.pontia.dev","port":8443,"acme_challenge":"dns-01","browser_dashboard_origin":"https://app.pontia.dev","preview_base_hostname":"cn.pontia.run"}"#;
        std::fs::write(&path, stored).unwrap();

        let config = ServiceConfig::read(&path).unwrap();

        assert_eq!(config.acme_challenge, AcmeChallenge::Dns01);
        assert_eq!(
            config.preview_base_hostname.as_deref(),
            Some("cn.pontia.run")
        );
        assert_eq!(std::fs::read(&path).unwrap(), stored);
    }

    #[test]
    fn ip_edges_default_to_a_preview_pool_and_can_disable_it() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config.json");
        std::fs::write(&path, br#"{"hostname":"8.8.8.8"}"#).unwrap();

        let config = ServiceConfig::read(&path).unwrap();
        let preview = config.port_preview.enabled().unwrap();
        assert_eq!(preview.ranges, ["20000-20099"]);
        assert_eq!(preview.max_listeners, 32);
        assert_eq!(preview.allowed_ports(443).unwrap().len(), 100);

        std::fs::write(&path, br#"{"hostname":"8.8.8.8","port_preview":false}"#).unwrap();
        assert!(
            ServiceConfig::read(&path)
                .unwrap()
                .port_preview
                .enabled()
                .is_none()
        );
    }

    #[test]
    fn validates_preview_pool_ranges_and_listener_limit() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config.json");
        for invalid in [
            r#"{"hostname":"8.8.8.8","port_preview":true}"#,
            r#"{"hostname":"8.8.8.8","port_preview":{"ranges":[]}}"#,
            r#"{"hostname":"8.8.8.8","port_preview":{"ranges":["80"]}}"#,
            r#"{"hostname":"8.8.8.8","port_preview":{"ranges":["20001-20000"]}}"#,
            r#"{"hostname":"8.8.8.8","port_preview":{"ranges":["20000"],"max_listeners":129}}"#,
        ] {
            std::fs::write(&path, invalid).unwrap();
            assert!(ServiceConfig::read(&path).is_err(), "accepted {invalid}");
        }
    }

    #[test]
    fn rejects_non_public_configured_hostnames() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config.json");
        for config in [
            r#"{"hostname":"localhost"}"#,
            r#"{"hostname":"127.0.0.1"}"#,
            r#"{"hostname":"10.0.0.1"}"#,
            r#"{"hostname":"2001:db8::1"}"#,
            r#"{"hostname":"edge.internal"}"#,
            r#"{"hostname":"edge.example.com","preview_base_hostname":"preview.local"}"#,
        ] {
            std::fs::write(&path, config).unwrap();
            assert!(ServiceConfig::read(&path).is_err(), "accepted {config}");
        }
    }

    #[test]
    fn accepts_canonical_tunnel_urls_for_public_hostnames() {
        for (url, hostname) in [
            ("wss://edge.example.com/tunnel", "edge.example.com"),
            ("wss://edge.example.com:8443/tunnel", "edge.example.com"),
            (
                "wss://brave-atlas.edge.pontia.dev/tunnel",
                "brave-atlas.edge.pontia.dev",
            ),
            ("wss://relay.pontia.run/tunnel", "relay.pontia.run"),
            ("wss://8.8.8.8/tunnel", "8.8.8.8"),
            (
                "wss://[2606:4700:4700::1111]:8443/tunnel",
                "2606:4700:4700::1111",
            ),
        ] {
            assert_eq!(hostname_from_tunnel_url(url).unwrap(), hostname);
        }
        for invalid in [
            "wss://edge.example.com:25/tunnel",
            "wss://edge.example.com:443/tunnel",
            "wss://Edge.example.com/tunnel",
            "ws://edge.example.com/tunnel",
            "wss://edge.example.com/other",
            "wss://edge.example.com/tunnel?q=1",
            "wss://127.0.0.1/tunnel",
            "wss://[2001:db8::1]/tunnel",
            "wss://localhost/tunnel",
            "wss://edge.local/tunnel",
        ] {
            assert!(
                hostname_from_tunnel_url(invalid).is_err(),
                "accepted {invalid}"
            );
        }
    }
}
