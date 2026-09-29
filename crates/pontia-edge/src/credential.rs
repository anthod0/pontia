use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::Path,
};

use anyhow::{Context, Result};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use uuid::Uuid;

pub const CREDENTIAL_PATH: &str = "/etc/pontia/edge/credential";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EdgeCredential {
    pub edge_id: Uuid,
    pub value: String,
}

pub fn ensure_root(effective_user_id: u32) -> Result<()> {
    anyhow::ensure!(
        effective_user_id == 0,
        "pontia-edge init must be run as root"
    );
    Ok(())
}

pub fn initialize_credential(path: &Path, edge_id: Uuid) -> Result<EdgeCredential> {
    if path.exists() {
        anyhow::bail!("edge credential file already exists at {}", path.display());
    }
    let credential = generate_credential(edge_id)?;
    save_credential(path, &credential.value)?;
    Ok(credential)
}

pub fn replace_credential(path: &Path, edge_id: Uuid) -> Result<EdgeCredential> {
    let credential = generate_credential(edge_id)?;
    save_credential(path, &credential.value)?;
    Ok(credential)
}

pub fn read_edge_credential(path: &Path) -> Result<EdgeCredential> {
    let contents = fs::read_to_string(path)
        .with_context(|| format!("failed to read edge credential from {}", path.display()))?;
    let value = contents
        .strip_suffix('\n')
        .context("edge credential file must end with a newline")?;
    let edge_id = parse_credential(value)
        .map(|(edge_id, _)| edge_id)
        .context("edge credential file contains an invalid credential")?;
    Ok(EdgeCredential {
        edge_id,
        value: value.to_owned(),
    })
}

pub fn valid_credential(value: &str) -> bool {
    parse_credential(value).is_some()
}

fn generate_credential(edge_id: Uuid) -> Result<EdgeCredential> {
    anyhow::ensure!(edge_id.get_version_num() == 7, "edge ID must be a UUID v7");
    let mut secret_bytes = [0_u8; 32];
    getrandom::fill(&mut secret_bytes).context("failed to generate edge credential")?;
    let secret = URL_SAFE_NO_PAD.encode(secret_bytes);
    secret_bytes.fill(0);

    Ok(EdgeCredential {
        edge_id,
        value: format!("pec_v1_{edge_id}_{secret}"),
    })
}

fn save_credential(path: &Path, credential: &str) -> Result<()> {
    let parent = path
        .parent()
        .context("edge credential path has no parent directory")?;
    fs::create_dir_all(parent).with_context(|| format!("failed to create {}", parent.display()))?;

    let mut random = [0_u8; 8];
    getrandom::fill(&mut random).context("failed to create credential temporary file")?;
    let temporary_path = parent.join(format!(
        ".credential.{}.tmp",
        URL_SAFE_NO_PAD.encode(random)
    ));
    let result = (|| -> Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary_path)
            .with_context(|| {
                format!(
                    "failed to create credential temporary file in {}",
                    parent.display()
                )
            })?;
        file.write_all(credential.as_bytes())?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        fs::set_permissions(&temporary_path, fs::Permissions::from_mode(0o600))?;
        fs::rename(&temporary_path, path)
            .with_context(|| format!("failed to save edge credential at {}", path.display()))?;
        File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary_path);
    }
    result
}

fn parse_credential(value: &str) -> Option<(Uuid, [u8; 32])> {
    let remainder = value.strip_prefix("pec_v1_")?;
    let (edge_id_text, secret) = remainder.split_once('_')?;
    if secret.len() != 43 {
        return None;
    }

    let edge_id = Uuid::parse_str(edge_id_text).ok()?;
    if edge_id.get_version_num() != 7 || edge_id.to_string() != edge_id_text {
        return None;
    }

    let decoded = URL_SAFE_NO_PAD.decode(secret).ok()?;
    let secret_bytes: [u8; 32] = decoded.try_into().ok()?;
    if URL_SAFE_NO_PAD.encode(secret_bytes) != secret {
        return None;
    }
    Some((edge_id, secret_bytes))
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    fn edge_id() -> Uuid {
        Uuid::parse_str("0199791c-6600-7000-8000-000000000001").unwrap()
    }

    #[test]
    fn initialization_saves_a_valid_credential_without_overwriting() {
        let test_root = tempfile::tempdir().expect("create isolated test root");
        let path = test_root.path().join("etc/pontia/edge/credential");

        let credential = initialize_credential(&path, edge_id()).expect("initialize credential");

        assert_eq!(credential.edge_id, edge_id());
        assert!(valid_credential(&credential.value));
        assert_eq!(read_edge_credential(&path).unwrap(), credential);
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(initialize_credential(&path, edge_id()).is_err());
        assert_eq!(read_edge_credential(&path).unwrap(), credential);
    }

    #[test]
    fn replacement_is_atomic_and_uses_the_requested_edge_id() {
        let test_root = tempfile::tempdir().expect("create isolated test root");
        let path = test_root.path().join("credential");
        let first = initialize_credential(&path, edge_id()).unwrap();
        let replacement_id = Uuid::parse_str("0199791c-6600-7000-8000-000000000002").unwrap();

        let replacement = replace_credential(&path, replacement_id).unwrap();

        assert_ne!(replacement.value, first.value);
        assert_eq!(read_edge_credential(&path).unwrap(), replacement);
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[test]
    fn credential_validation_is_strict() {
        let test_root = tempfile::tempdir().expect("create isolated test root");
        let credential = initialize_credential(&test_root.path().join("credential"), edge_id())
            .unwrap()
            .value;

        for invalid in [
            format!(" {credential}"),
            format!("{credential} "),
            format!("{credential}_extra"),
            credential.replace("pec_v1_", "pec_v2_"),
            credential.replace('-', "+"),
        ] {
            assert!(!valid_credential(&invalid), "accepted {invalid:?}");
        }
    }

    #[test]
    fn root_check_rejects_non_root_users() {
        assert!(ensure_root(0).is_ok());
        assert_eq!(
            ensure_root(1000).unwrap_err().to_string(),
            "pontia-edge init must be run as root"
        );
    }
}
