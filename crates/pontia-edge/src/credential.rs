use std::{
    fs::{self, OpenOptions},
    io::Write,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::Path,
};

use anyhow::{Context, Result};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use uuid::Uuid;

pub const CREDENTIAL_PATH: &str = "/etc/pontia/edge/credential";

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

pub fn initialize_credential(path: &Path) -> Result<EdgeCredential> {
    if path.exists() {
        anyhow::bail!("edge credential file already exists at {}", path.display());
    }

    let parent = path
        .parent()
        .context("edge credential path has no parent directory")?;
    fs::create_dir_all(parent).with_context(|| format!("failed to create {}", parent.display()))?;

    let credential = generate_credential()?;
    write_credential(path, &credential.value)?;
    Ok(credential)
}

pub fn read_credential(path: &Path) -> Result<String> {
    let contents = fs::read_to_string(path)
        .with_context(|| format!("failed to read edge credential from {}", path.display()))?;
    let value = contents
        .strip_suffix('\n')
        .context("edge credential file must end with a newline")?;
    anyhow::ensure!(
        parse_credential(value).is_some(),
        "edge credential file contains an invalid credential"
    );
    Ok(value.to_owned())
}

pub fn valid_credential(value: &str) -> bool {
    parse_credential(value).is_some()
}

fn generate_credential() -> Result<EdgeCredential> {
    let edge_id = Uuid::now_v7();
    let mut secret_bytes = [0_u8; 32];
    getrandom::fill(&mut secret_bytes).context("failed to generate edge credential")?;
    let secret = URL_SAFE_NO_PAD.encode(secret_bytes);
    secret_bytes.fill(0);

    Ok(EdgeCredential {
        edge_id,
        value: format!("pec_v1_{edge_id}_{secret}"),
    })
}

fn write_credential(path: &Path, credential: &str) -> Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .with_context(|| {
            format!(
                "failed to create edge credential file at {}",
                path.display()
            )
        })?;
    let result = file
        .write_all(credential.as_bytes())
        .and_then(|()| file.write_all(b"\n"))
        .and_then(|()| file.sync_all())
        .and_then(|()| fs::set_permissions(path, fs::Permissions::from_mode(0o600)))
        .with_context(|| format!("failed to save edge credential at {}", path.display()));
    if result.is_err() {
        let _ = fs::remove_file(path);
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

    #[test]
    fn initialization_saves_a_valid_credential_without_overwriting() {
        let test_root = tempfile::tempdir().expect("create isolated test root");
        let path = test_root.path().join("etc/pontia/edge/credential");

        let credential = initialize_credential(&path).expect("initialize credential");

        assert_eq!(credential.edge_id.get_version_num(), 7);
        assert!(valid_credential(&credential.value));
        assert_eq!(read_credential(&path).unwrap(), credential.value);
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(initialize_credential(&path).is_err());
        assert_eq!(read_credential(&path).unwrap(), credential.value);
    }

    #[test]
    fn credential_validation_is_strict() {
        let test_root = tempfile::tempdir().expect("create isolated test root");
        let credential = initialize_credential(&test_root.path().join("credential"))
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
