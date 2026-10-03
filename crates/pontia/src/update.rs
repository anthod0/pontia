use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

use base64::{Engine, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signature, VerifyingKey, pkcs8::DecodePublicKey};
use flate2::read::GzDecoder;
use fs2::FileExt;
use serde::Deserialize;
use sha2::{Digest, Sha256};

const ORIGIN: &str = "https://get.pontia.dev";
const BINARIES: [&str; 2] = ["pontia", "pontiad"];
const MAX_MANIFEST: u64 = 1024 * 1024;
const MAX_ARCHIVE: u64 = 512 * 1024 * 1024;

#[derive(Deserialize)]
struct Envelope {
    manifest: String,
    signature: String,
}

#[derive(Deserialize)]
struct Manifest {
    schema_version: u32,
    version: String,
    artifacts: HashMap<String, Artifact>,
}

#[derive(Deserialize)]
struct Artifact {
    url: String,
    sha256: String,
    size: u64,
}

pub struct PreparedUpdate {
    directory: PathBuf,
    staging: tempfile::TempDir,
    version: String,
    // Keep the lock inode in place, even after releasing the lock.
    _lock: File,
}

pub async fn prepare() -> Result<PreparedUpdate, String> {
    let target = target()?;
    let key = option_env!("PONTIA_RELEASE_PUBLIC_KEY")
        .ok_or("this build has no release verification key; use an official Pontia release")?
        .replace("\\n", "\n");
    let key = VerifyingKey::from_public_key_pem(&key)
        .map_err(|error| format!("invalid embedded release public key: {error}"))?;
    let executable = std::env::current_exe()
        .and_then(fs::canonicalize)
        .map_err(io_error)?;
    let directory = executable
        .parent()
        .ok_or("pontia has no parent directory")?;
    if executable.file_name().and_then(|name| name.to_str()) != Some("pontia") {
        return Err("update requires the executable to be named pontia".into());
    }
    let lock = lock_directory(directory)?;
    for binary in BINARIES {
        let metadata = fs::symlink_metadata(directory.join(binary)).map_err(io_error)?;
        if !metadata.file_type().is_file() {
            return Err(format!(
                "{binary} must be a regular file in {}",
                directory.display()
            ));
        }
    }
    let staging = tempfile::Builder::new()
        .prefix(".pontia-update-")
        .tempdir_in(directory)
        .map_err(io_error)?;
    let client = reqwest::Client::builder()
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(300))
        .build()
        .map_err(|error| error.to_string())?;
    println!("Checking the stable release at {ORIGIN}...");
    let envelope = download(
        &client,
        &format!("{ORIGIN}/channels/stable.json"),
        MAX_MANIFEST,
    )
    .await?;
    let manifest = verify_manifest(&envelope, &key)?;
    for binary in BINARIES {
        let name = format!("{binary}-{target}.tar.gz");
        let artifact = manifest
            .artifacts
            .get(&name)
            .ok_or_else(|| format!("release does not contain {name}"))?;
        let url = format!("{ORIGIN}/releases/{}/{name}", manifest.version);
        if artifact.url != url || artifact.size == 0 || artifact.size > MAX_ARCHIVE {
            return Err(format!("invalid release artifact: {name}"));
        }
        println!("Downloading {binary} {}...", manifest.version);
        let bytes = download(&client, &url, artifact.size).await?;
        verify_artifact(&bytes, artifact)?;
        extract_binary(&bytes, binary, &staging.path().join(binary))?;
    }
    Ok(PreparedUpdate {
        directory: directory.to_path_buf(),
        staging,
        version: manifest.version,
        _lock: lock,
    })
}

fn target() -> Result<&'static str, String> {
    if !cfg!(target_env = "gnu") {
        return Err("pontia update requires a GNU/Linux release build".into());
    }
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => Ok("x86_64-unknown-linux-gnu"),
        ("linux", "aarch64") => Ok("aarch64-unknown-linux-gnu"),
        _ => Err("pontia update supports only Linux x86_64 and aarch64".into()),
    }
}

fn lock_directory(directory: &Path) -> Result<File, String> {
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(directory.join(".pontia-update.lock"))
        .map_err(io_error)?;
    lock.try_lock_exclusive().map_err(|error| {
        format!("cannot lock the installation directory (another update may be running): {error}")
    })?;
    Ok(lock)
}

async fn download(client: &reqwest::Client, url: &str, limit: u64) -> Result<Vec<u8>, String> {
    let mut response = client
        .get(url)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|error| format!("download failed: {error}"))?;
    if response
        .content_length()
        .is_some_and(|length| length > limit)
    {
        return Err("download exceeds the release size limit".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|error| error.to_string())? {
        if bytes.len() as u64 + chunk.len() as u64 > limit {
            return Err("download exceeds the release size limit".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

fn verify_manifest(bytes: &[u8], key: &VerifyingKey) -> Result<Manifest, String> {
    let envelope: Envelope = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
    let payload = STANDARD
        .decode(envelope.manifest)
        .map_err(|error| error.to_string())?;
    let signature = STANDARD
        .decode(envelope.signature)
        .map_err(|error| error.to_string())?;
    let signature = Signature::from_slice(&signature).map_err(|error| error.to_string())?;
    key.verify_strict(&payload, &signature)
        .map_err(|_| "release signature verification failed".to_string())?;
    let manifest: Manifest = serde_json::from_slice(&payload).map_err(|error| error.to_string())?;
    let valid_version = manifest.version.strip_prefix('v').is_some_and(|version| {
        version.starts_with(|c: char| c.is_ascii_digit())
            && version
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b".+-".contains(&c))
    });
    if manifest.schema_version != 1 || !valid_version {
        return Err("invalid release manifest".into());
    }
    Ok(manifest)
}

fn verify_artifact(bytes: &[u8], artifact: &Artifact) -> Result<(), String> {
    let checksum = format!("{:x}", Sha256::digest(bytes));
    if bytes.len() as u64 != artifact.size || checksum != artifact.sha256 {
        return Err("release artifact size or checksum verification failed".into());
    }
    Ok(())
}

fn extract_binary(bytes: &[u8], binary: &str, destination: &Path) -> Result<(), String> {
    let mut archive = tar::Archive::new(GzDecoder::new(bytes));
    let mut found = false;
    for entry in archive.entries().map_err(io_error)? {
        let mut entry = entry.map_err(io_error)?;
        if found
            || entry.path().map_err(io_error)?.as_ref() != Path::new(binary)
            || !entry.header().entry_type().is_file()
            || entry.size() > MAX_ARCHIVE
        {
            return Err(format!("invalid archive for {binary}"));
        }
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(destination)
            .map_err(io_error)?;
        std::io::copy(&mut entry.by_ref().take(MAX_ARCHIVE + 1), &mut file).map_err(io_error)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(fs::Permissions::from_mode(0o755))
                .map_err(io_error)?;
        }
        file.flush()
            .and_then(|_| file.sync_all())
            .map_err(io_error)?;
        found = true;
    }
    if !found {
        return Err(format!("release archive does not contain {binary}"));
    }
    Ok(())
}

impl PreparedUpdate {
    pub fn daemon_path(&self) -> PathBuf {
        self.directory.join("pontiad")
    }

    #[cfg(target_os = "linux")]
    pub fn ensure_no_unmanaged_daemon(&self) -> Result<(), String> {
        ensure_not_running(&self.daemon_path())
    }

    pub fn install(self, mut restart: impl FnMut() -> Result<(), String>) -> Result<(), String> {
        // Hard links retain both original binaries without renaming/removing the live paths.
        for binary in BINARIES {
            fs::hard_link(
                self.directory.join(binary),
                self.staging.path().join(format!("{binary}.old")),
            )
            .map_err(io_error)?;
        }
        let mut replaced = Vec::new();
        let outcome = (|| {
            for binary in BINARIES {
                fs::rename(
                    self.staging.path().join(binary),
                    self.directory.join(binary),
                )
                .map_err(io_error)?;
                replaced.push(binary);
            }
            restart()
        })();
        if let Err(error) = outcome {
            for binary in replaced {
                if let Err(rollback) = fs::rename(
                    self.staging.path().join(format!("{binary}.old")),
                    self.directory.join(binary),
                ) {
                    let backup = self.staging.keep();
                    return Err(format!(
                        "{error}; rollback failed: {rollback}; original binaries retained at {}",
                        backup.display()
                    ));
                }
            }
            let recovery = restart();
            return Err(match recovery {
                Ok(()) => format!("{error}; restored the previous binaries"),
                Err(recovery) => format!(
                    "{error}; restored the previous binaries, but service recovery failed: {recovery}"
                ),
            });
        }
        println!("Updated pontia and pontiad to {}.", self.version);
        Ok(())
    }
}

#[cfg(target_os = "linux")]
fn ensure_not_running(executable: &Path) -> Result<(), String> {
    let deleted = PathBuf::from(format!("{} (deleted)", executable.display()));
    for process in fs::read_dir("/proc").map_err(io_error)? {
        let process = process.map_err(io_error)?;
        if !process
            .file_name()
            .to_string_lossy()
            .bytes()
            .all(|c| c.is_ascii_digit())
        {
            continue;
        }
        // Processes may exit during enumeration, and other users' executables may be private.
        if let Ok(path) = fs::read_link(process.path().join("exe"))
            && (path == executable || path == deleted)
        {
            return Err("an unmanaged pontiad is running from this installation; stop it before updating and restart it afterwards".into());
        }
    }
    Ok(())
}

fn io_error(error: std::io::Error) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use flate2::{Compression, write::GzEncoder};
    use serde_json::json;

    fn envelope(key: &SigningKey, payload: &[u8]) -> Vec<u8> {
        serde_json::to_vec(&json!({
            "manifest": STANDARD.encode(payload),
            "signature": STANDARD.encode(key.sign(payload).to_bytes()),
        }))
        .unwrap()
    }

    #[test]
    fn verifies_signed_manifests_and_rejects_tampering_and_unsafe_versions() {
        let key = SigningKey::from_bytes(&[7; 32]);
        let payload = br#"{"schema_version":1,"version":"v1.2.3","artifacts":{}}"#;
        let signed = envelope(&key, payload);
        assert_eq!(
            verify_manifest(&signed, &key.verifying_key())
                .unwrap()
                .version,
            "v1.2.3"
        );
        let other = SigningKey::from_bytes(&[8; 32]);
        assert!(verify_manifest(&signed, &other.verifying_key()).is_err());
        let mut tampered: serde_json::Value = serde_json::from_slice(&signed).unwrap();
        tampered["manifest"] = json!(STANDARD.encode(b"tampered"));
        assert!(
            verify_manifest(
                &serde_json::to_vec(&tampered).unwrap(),
                &key.verifying_key()
            )
            .is_err()
        );
        for (schema, version) in [(2, "v1.2.3"), (1, "v1/../../other"), (1, "latest")] {
            let payload = serde_json::to_vec(
                &json!({"schema_version": schema, "version": version, "artifacts": {}}),
            )
            .unwrap();
            assert!(verify_manifest(&envelope(&key, &payload), &key.verifying_key()).is_err());
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn detects_an_executable_running_outside_the_service_manager() {
        assert!(ensure_not_running(&std::env::current_exe().unwrap()).is_err());
        let root = tempfile::tempdir().unwrap();
        assert!(ensure_not_running(&root.path().join("pontiad")).is_ok());
    }

    #[test]
    fn artifact_verification_checks_size_and_checksum() {
        let bytes = b"release";
        let mut artifact = Artifact {
            url: String::new(),
            size: bytes.len() as u64,
            sha256: format!("{:x}", Sha256::digest(bytes)),
        };
        verify_artifact(bytes, &artifact).unwrap();
        assert!(verify_artifact(b"changed", &artifact).is_err());
        artifact.size += 1;
        assert!(verify_artifact(bytes, &artifact).is_err());
    }

    fn archive(name: &str, kind: tar::EntryType) -> Vec<u8> {
        let encoder = GzEncoder::new(Vec::new(), Compression::default());
        let mut builder = tar::Builder::new(encoder);
        let mut header = tar::Header::new_gnu();
        header.set_size(7);
        header.set_mode(0o755);
        header.set_entry_type(kind);
        header.set_cksum();
        builder
            .append_data(&mut header, name, &b"release"[..])
            .unwrap();
        builder.into_inner().unwrap().finish().unwrap()
    }

    #[test]
    fn extracts_only_the_expected_regular_binary() {
        let root = tempfile::tempdir().unwrap();
        let output = root.path().join("pontia");
        extract_binary(
            &archive("pontia", tar::EntryType::Regular),
            "pontia",
            &output,
        )
        .unwrap();
        assert_eq!(fs::read(&output).unwrap(), b"release");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&output).unwrap().permissions().mode() & 0o777,
                0o755
            );
        }
        assert!(
            extract_binary(
                &archive("other", tar::EntryType::Regular),
                "pontia",
                &root.path().join("wrong")
            )
            .is_err()
        );
        assert!(
            extract_binary(
                &archive("pontia", tar::EntryType::Symlink),
                "pontia",
                &root.path().join("link")
            )
            .is_err()
        );
        assert!(!root.path().join("link").exists());
    }

    fn prepared(root: &Path) -> PreparedUpdate {
        for binary in BINARIES {
            fs::write(root.join(binary), format!("old {binary}")).unwrap();
        }
        let staging = tempfile::tempdir_in(root).unwrap();
        for binary in BINARIES {
            fs::write(staging.path().join(binary), format!("new {binary}")).unwrap();
        }
        PreparedUpdate {
            directory: root.into(),
            staging,
            version: "v1.2.3".into(),
            _lock: lock_directory(root).unwrap(),
        }
    }

    #[test]
    fn installs_both_binaries_before_restarting_and_releases_lock() {
        let root = tempfile::tempdir().unwrap();
        let update = prepared(root.path());
        assert!(lock_directory(root.path()).is_err());
        let mut calls = 0;
        update
            .install(|| {
                calls += 1;
                for binary in BINARIES {
                    assert_eq!(
                        fs::read_to_string(root.path().join(binary)).unwrap(),
                        format!("new {binary}")
                    );
                }
                Ok(())
            })
            .unwrap();
        assert_eq!(calls, 1);
        assert!(lock_directory(root.path()).is_ok());
    }

    #[test]
    fn restores_both_binaries_and_restarts_old_service_when_health_check_fails() {
        let root = tempfile::tempdir().unwrap();
        let update = prepared(root.path());
        let mut calls = 0;
        let error = update
            .install(|| {
                calls += 1;
                if calls == 1 {
                    return Err("unhealthy".into());
                }
                for binary in BINARIES {
                    assert_eq!(
                        fs::read_to_string(root.path().join(binary)).unwrap(),
                        format!("old {binary}")
                    );
                }
                Ok(())
            })
            .unwrap_err();
        assert_eq!(calls, 2);
        assert!(error.contains("restored the previous binaries"));
    }

    #[test]
    fn restores_first_binary_when_second_replacement_fails() {
        let root = tempfile::tempdir().unwrap();
        let update = prepared(root.path());
        fs::remove_file(update.staging.path().join("pontiad")).unwrap();
        assert!(update.install(|| Ok(())).is_err());
        for binary in BINARIES {
            assert_eq!(
                fs::read_to_string(root.path().join(binary)).unwrap(),
                format!("old {binary}")
            );
        }
    }
}
