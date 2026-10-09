use std::{fs, path::Path};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{acme::AcmeChallenge, files::atomic_write, port::validate_edge_port};

pub const PENDING_INIT_PATH: &str = "/etc/pontia/edge/pending-init.json";

#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PendingInit {
    pub edge_id: Uuid,
    pub ticket: String,
    pub acme_staging: bool,
    pub port: Option<u16>,
    pub acme_challenge: AcmeChallenge,
}

impl PendingInit {
    pub fn save(&self, path: &Path) -> Result<()> {
        let mut contents =
            serde_json::to_vec(self).context("failed to serialize pending initialization")?;
        contents.push(b'\n');
        atomic_write(path, &contents, 0o600).context("failed to save pending initialization")
    }

    pub fn read(path: &Path) -> Result<Self> {
        let pending: Self =
            serde_json::from_slice(&fs::read(path).with_context(|| {
                format!("no pending initialization found at {}", path.display())
            })?)
            .context("pending initialization is invalid")?;
        anyhow::ensure!(
            pending.edge_id.get_version_num() == 7,
            "pending initialization contains an invalid edge ID"
        );
        if let Some(port) = pending.port {
            validate_edge_port(port)
                .map_err(anyhow::Error::msg)
                .context("pending initialization contains an invalid port")?;
        }
        Ok(pending)
    }
}

pub fn remove(path: &Path) -> Result<()> {
    fs::remove_file(path).with_context(|| {
        format!(
            "failed to remove pending initialization at {}",
            path.display()
        )
    })
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    fn parameters() -> PendingInit {
        PendingInit {
            edge_id: Uuid::parse_str("0199791c-6600-7000-8000-000000000001").unwrap(),
            ticket: "pet_v1_secret".to_owned(),
            acme_staging: true,
            port: Some(8443),
            acme_challenge: AcmeChallenge::Dns01,
        }
    }

    #[test]
    fn pending_parameters_round_trip_in_a_private_file() {
        let test_root = tempfile::tempdir().unwrap();
        let path = test_root.path().join("pending-init.json");
        let expected = parameters();

        expected.save(&path).unwrap();

        assert!(PendingInit::read(&path).unwrap() == expected);
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[test]
    fn replacing_pending_parameters_is_atomic() {
        let test_root = tempfile::tempdir().unwrap();
        let path = test_root.path().join("pending-init.json");
        parameters().save(&path).unwrap();
        let mut replacement = parameters();
        replacement.ticket = "pet_v1_replacement".to_owned();

        replacement.save(&path).unwrap();

        assert!(PendingInit::read(&path).unwrap() == replacement);
    }

    #[test]
    fn removed_parameters_cannot_be_retried() {
        let test_root = tempfile::tempdir().unwrap();
        let path = test_root.path().join("pending-init.json");
        parameters().save(&path).unwrap();

        remove(&path).unwrap();

        assert!(PendingInit::read(&path).is_err());
    }
}
